//! Authenticated, bounded photo uploads. Decode and re-encode to discard metadata
//! and arbitrary trailing content; never fetch an operator-supplied remote URL.
use crate::state::{AppState, CurrentAdmin};
use axum::{
    body::Bytes,
    extract::{DefaultBodyLimit, Path, State},
    http::header,
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use image::{ImageFormat, ImageReader, Limits};
use serde_json::{json, Value};
use sn_core::{Error, Result};
use sqlx::Row;
use std::io::Cursor;
use uuid::Uuid;

const MAX_BYTES: usize = 10 * 1024 * 1024;
static DECODERS: tokio::sync::Semaphore = tokio::sync::Semaphore::const_new(2);

pub fn routes() -> Router<AppState> {
    Router::new()
        .route(
            "/api/broadcasts/media",
            post(upload).layer(DefaultBodyLimit::max(MAX_BYTES)),
        )
        .route("/api/broadcasts/media/{id}", get(download))
}
struct Photo {
    data: Vec<u8>,
    content_type: &'static str,
    width: u32,
    height: u32,
}

fn normalize(data: &[u8]) -> Result<Photo> {
    if data.is_empty() || data.len() > MAX_BYTES {
        return Err(Error::bad("Фото должно быть не больше 10 МБ"));
    }
    let format =
        image::guess_format(data).map_err(|_| Error::bad("Выберите фото JPG, PNG или WebP"))?;
    if !matches!(
        format,
        ImageFormat::Jpeg | ImageFormat::Png | ImageFormat::WebP
    ) {
        return Err(Error::bad("Выберите фото JPG, PNG или WebP"));
    }
    let (width, height) = ImageReader::with_format(Cursor::new(data), format)
        .into_dimensions()
        .map_err(|_| Error::bad("Не удалось прочитать фото: файл повреждён"))?;
    if width == 0
        || height == 0
        || width.saturating_add(height) > 10000
        || width.max(height) > width.min(height).saturating_mul(20)
        || u64::from(width) * u64::from(height) > 16_000_000
    {
        return Err(Error::bad(
            "Фото: до 16 Мп, сумма сторон до 10000 пикселей, соотношение сторон до 20:1",
        ));
    }
    let mut reader = ImageReader::with_format(Cursor::new(data), format);
    let mut limits = Limits::default();
    limits.max_image_width = Some(10000);
    limits.max_image_height = Some(10000);
    limits.max_alloc = Some(128 * 1024 * 1024);
    reader.limits(limits);
    let mut decoded = reader
        .decode()
        .map_err(|_| Error::bad("Не удалось прочитать фото: файл повреждён или слишком большой"))?;
    // Apply JPEG orientation before stripping EXIF, so phone photos stay upright.
    if let Ok(mut decoder) = ImageReader::with_format(Cursor::new(data), format).into_decoder() {
        use image::ImageDecoder;
        if let Ok(orientation) = decoder.orientation() {
            decoded.apply_orientation(orientation);
        }
    }
    let (output, content_type) = if decoded.color().has_alpha() {
        (ImageFormat::Png, "image/png")
    } else {
        (ImageFormat::Jpeg, "image/jpeg")
    };
    let mut out = Cursor::new(Vec::new());
    decoded
        .write_to(&mut out, output)
        .map_err(|_| Error::bad("Не удалось подготовить фото"))?;
    let data = out.into_inner();
    if data.len() > MAX_BYTES {
        return Err(Error::bad(
            "Подготовленное фото больше 10 МБ. Уменьшите изображение",
        ));
    }
    Ok(Photo {
        data,
        content_type,
        width: decoded.width(),
        height: decoded.height(),
    })
}

async fn upload(
    CurrentAdmin(admin): CurrentAdmin,
    State(st): State<AppState>,
    data: Bytes,
) -> Result<Json<Value>> {
    let permit = DECODERS
        .try_acquire()
        .map_err(|_| Error::bad("Фото обрабатываются. Повторите через несколько секунд"))?;
    let photo = tokio::task::spawn_blocking(move || {
        let _permit = permit;
        normalize(&data)
    })
    .await
    .map_err(|_| Error::Internal("photo decoder failed".into()))??;
    let mut tx = st.pool.begin().await?;
    // Serialize the per-operator quota; abandoned uploads expire after a day.
    sqlx::query("SELECT id FROM admins WHERE id=$1 FOR UPDATE")
        .bind(admin.id)
        .fetch_one(&mut *tx)
        .await?;
    sqlx::query("DELETE FROM broadcast_media m WHERE created_by=$1 AND created_at<now()-interval '1 day' AND NOT EXISTS(SELECT 1 FROM broadcasts b WHERE b.photo_id=m.id)")
        .bind(admin.id).execute(&mut *tx).await?;
    let recent: i64 = sqlx::query_scalar("SELECT count(*) FROM broadcast_media WHERE created_by=$1 AND created_at>now()-interval '5 minutes'")
        .bind(admin.id).fetch_one(&mut *tx).await?;
    if recent >= 30 {
        return Err(Error::bad(
            "Слишком много загрузок фото. Повторите через 5 минут",
        ));
    }
    let id = Uuid::new_v4();
    let size = photo.data.len();
    sqlx::query("INSERT INTO broadcast_media(id,content_type,data,width,height,created_by) VALUES($1,$2,$3,$4,$5,$6)")
        .bind(id).bind(photo.content_type).bind(photo.data).bind(photo.width as i32).bind(photo.height as i32).bind(admin.id).execute(&mut *tx).await?;
    tx.commit().await?;
    Ok(Json(
        json!({"id":id,"content_type":photo.content_type,"size":size,"width":photo.width,"height":photo.height}),
    ))
}
async fn download(
    _a: CurrentAdmin,
    State(st): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Response> {
    let row = sqlx::query("SELECT content_type,data FROM broadcast_media WHERE id=$1")
        .bind(id)
        .fetch_optional(&st.pool)
        .await?
        .ok_or(Error::NotFound)?;
    Ok((
        [
            (header::CONTENT_TYPE, row.get::<String, _>("content_type")),
            (header::CACHE_CONTROL, "private, no-store".into()),
            (header::X_CONTENT_TYPE_OPTIONS, "nosniff".into()),
        ],
        row.get::<Vec<u8>, _>("data"),
    )
        .into_response())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn verifies_content_and_strips_trailing_data() {
        for format in [ImageFormat::Jpeg, ImageFormat::Png, ImageFormat::WebP] {
            let mut fixture = Cursor::new(Vec::new());
            image::DynamicImage::new_rgb8(30, 20)
                .write_to(&mut fixture, format)
                .unwrap();
            assert!(normalize(fixture.get_ref()).is_ok(), "{format:?}");
        }
        let mut out = Cursor::new(Vec::new());
        image::DynamicImage::new_rgb8(30, 20)
            .write_to(&mut out, ImageFormat::Png)
            .unwrap();
        out.get_mut()
            .extend_from_slice(b"<script>metadata must not survive</script>");
        let photo = normalize(out.get_ref()).unwrap();
        assert_eq!((photo.width, photo.height), (30, 20));
        assert!(!photo.data.windows(6).any(|s| s == b"script"));
        assert!(normalize(b"<svg onload='alert(1)'></svg>").is_err());
        assert!(normalize(b"\x89PNG\r\n\x1a\ncorrupt").is_err());
        assert!(normalize(&vec![0; MAX_BYTES + 1]).is_err());
        let mut out = Cursor::new(Vec::new());
        image::DynamicImage::new_rgb8(500, 1)
            .write_to(&mut out, ImageFormat::Png)
            .unwrap();
        assert!(normalize(out.get_ref()).is_err());
    }
}
