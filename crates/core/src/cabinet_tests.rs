use super::*;

#[tokio::test]
#[ignore = "requires SN_TEST_DATABASE_URL pointing to an isolated sn_audit database"]
async fn telegram_link_keeps_paid_identity_and_rejects_replay_conflicts_and_revocation() -> Result<()> {
    let pool=crate::db::connect(&std::env::var("SN_TEST_DATABASE_URL").expect("isolated test database")).await?;
    let db:String=sqlx::query_scalar("SELECT current_database()").fetch_one(&pool).await?;
    assert!(db.starts_with("sn_audit_"));
    async fn client(pool:&Pool,tg:bool,used:bool)->Result<i64>{
        let id:i64=sqlx::query_scalar("INSERT INTO clients(username,short_id) VALUES('Cabinet link test',$1) RETURNING id").bind(uuid::Uuid::new_v4().to_string()).fetch_one(pool).await?;
        if tg {sqlx::query("INSERT INTO client_identities(client_id,kind,value,is_verified) VALUES($1,'telegram',$2,true)").bind(id).bind(format!("90000000{id}")).execute(pool).await?;}
        if used {sqlx::query("INSERT INTO subscriptions(client_id,expires_at) VALUES($1,now()+interval '30 days')").bind(id).execute(pool).await?;}
        Ok(id)
    }
    let installation:uuid::Uuid=sqlx::query_scalar("INSERT INTO cabinet_installations(name,public_url,server_ip,placement) VALUES('Link QA',$1,'192.0.2.1','separate') RETURNING id").bind(format!("https://{}.example.test",uuid::Uuid::new_v4())).fetch_one(&pool).await?;
    let mut clients=vec![];
    for (site_used,tg_used) in [(true,false),(false,true),(true,true)] {
        let site=client(&pool,false,site_used).await?;let tg=client(&pool,true,tg_used).await?;clients.extend([site,tg]);
        let code=issue_from_telegram(&pool,site,false).await?;
        let link=create_link(&pool,site,installation).await?;
        if site_used&&tg_used {assert!(confirm_link(&pool,tg,&link).await.is_err());continue;}
        let expected=if tg_used {tg} else {site};
        assert_eq!(confirm_link(&pool,tg,&link).await?,expected);
        let owner:i64=sqlx::query_scalar("SELECT client_id FROM cabinet_credentials WHERE code_hash=$1").bind(code_hash(&code).unwrap()).fetch_one(&pool).await?;
        assert_eq!(owner,expected);
        let identity:i64=sqlx::query_scalar("SELECT client_id FROM client_identities WHERE kind='telegram' AND value=$1").bind(format!("90000000{tg}")).fetch_one(&pool).await?;
        assert_eq!(identity,expected);
        assert!(confirm_link(&pool,tg,&link).await.is_err(),"used challenge cannot be replayed");
    }
    let site=client(&pool,false,false).await?;let tg=client(&pool,true,false).await?;clients.extend([site,tg]);
    issue_from_telegram(&pool,site,false).await?;
    let expired=create_link(&pool,site,installation).await?;
    sqlx::query("UPDATE cabinet_links SET expires_at=now()-interval '1 second' WHERE token_hash=$1").bind(crate::auth::token_hash(&expired)).execute(&pool).await?;
    assert!(confirm_link(&pool,tg,&expired).await.is_err());
    let revoked=create_link(&pool,site,installation).await?;
    sqlx::query("UPDATE cabinet_installations SET revoked_at=now() WHERE id=$1").bind(installation).execute(&pool).await?;
    assert!(confirm_link(&pool,tg,&revoked).await.is_err());
    sqlx::query("DELETE FROM cabinet_links WHERE installation_id=$1").bind(installation).execute(&pool).await?;
    sqlx::query("DELETE FROM cabinet_installations WHERE id=$1").bind(installation).execute(&pool).await?;
    sqlx::query("DELETE FROM cabinet_credentials WHERE client_id=ANY($1)").bind(&clients).execute(&pool).await?;
    sqlx::query("DELETE FROM clients WHERE id=ANY($1)").bind(clients).execute(&pool).await?;
    Ok(())
}
