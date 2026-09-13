[Все инструкции](README.md) · [Русский](../ru/backup-restore.md) / [English](../en/backup-restore.md)

# Резервное копирование и восстановление

Для восстановления нужны **база и соответствующий файл .env**. Без `CABINET_CODE_KEY` нельзя расшифровать сохранённые коды доступа. Сохраняйте также конфигурацию proxy и окружение отдельных служб.

## Копия стандартной установки

Команды выполняются от root на сервере с локальной базой `stealthnet`:

```bash
umask 077
sn_backup_dir="/root/stealthnet-backup-$(date -u +%Y%m%dT%H%M%SZ)"
mkdir "$sn_backup_dir"
runuser -u postgres -- pg_dump -Fc stealthnet > "$sn_backup_dir/database.dump"
cp /opt/stealthnet-software/.env "$sn_backup_dir/panel.env"
cp /opt/stealthnet-software/installation.json "$sn_backup_dir/installation.json"
cp -a /etc/caddy "$sn_backup_dir/caddy"
pg_restore --list "$sn_backup_dir/database.dump" > "$sn_backup_dir/contents.txt"
```

Перенесите закрытую копию вне этого сервера и проверьте восстановление в изолированную базу. Для внешней PostgreSQL используйте её штатное резервное копирование с учётом DATABASE_URL; не передавайте пароль в аргументах. Отдельно сохраните `/etc/sn-cabinet/env`, `/etc/sn-sub/env` и `/etc/sn-node/node.env` на соответствующих машинах.

## Восстановление стандартной установки

Ниже `/root/stealthnet-restore` — каталог с **выбранной проверенной копией**, а не автоматически последняя копия. Операция заменяет текущие данные; выполните её в окно обслуживания после дополнительной копии текущего состояния.

1. Проверьте `pg_restore --list` и совместимость версии PostgreSQL. Установите релиз приложения, соответствующий копии.
2. Остановите службы и восстановите базу/окружение:

```bash
set -e
systemctl stop sn-api sn-sub sn-worker sn-bot
runuser -u postgres -- pg_restore --exit-on-error --clean --if-exists --no-owner --role=stealthnet -d stealthnet < /root/stealthnet-restore/database.dump
install -m 600 /root/stealthnet-restore/panel.env /opt/stealthnet-software/.env
systemctl start sn-api sn-sub sn-worker
```

3. Запустите sn-bot только если его токен настроен. Проверьте `stealthnet doctor`, вход администратора, код клиента, подписку и реальное VPN-подключение.
4. Восстанавливайте Caddy выборочно после сравнения доменов; выполните `caddy validate` перед reload. Не затирайте сайты, созданные после копии.

`make update` автоматически сохраняет базу и .env перед обновлением. Откат бинарников после ошибки не отменяет уже применённые миграции и не заменяет проверку восстановления.
