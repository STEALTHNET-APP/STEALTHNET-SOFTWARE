# Security / Безопасность

## English

The project is in early release development. Use the current tested release and read its migration notes. There is no promise of vulnerability-free operation or maintenance of every old version.

Report vulnerabilities privately using **Security → Report a vulnerability** in the project repository when private reporting is enabled. If that action is unavailable, request a private contact through an issue **without exploit details, secrets or customer data**. Do not post a working exploit or credentials in a public bug template.

Include the affected version, deployment mode, required privileges, reproducible steps in an isolated environment, impact and a proposed fix if available. Redact tokens, access codes, subscription links, IPs and personal data. Do not test unrelated servers, access other customers' records or interrupt services to demonstrate a report.

Operators should retain database/environment backups, limit administrator and API-token privileges, enable 2FA, keep service ports private and verify signed payment events. Full-server outages require external monitoring. See [backup and restore](docs/en/backup-restore.md) and [troubleshooting](docs/en/troubleshooting.md).

## Русский

Проект находится на ранней стадии выпуска. Используйте актуальную проверенную версию и читайте примечания о миграциях. Отсутствие уязвимостей и сопровождение каждой старой версии не гарантируются.

Отправляйте уязвимости приватно через **Security → Report a vulnerability** репозитория, когда приватные отчёты включены. Если кнопки нет, запросите приватный контакт в issue **без описания эксплуатации, секретов и клиентских данных**. Не публикуйте работающий эксплойт или учётные данные в обычном отчёте о баге.

Укажите версию, вариант установки, необходимые права, воспроизведение в изолированном окружении, последствия и возможное исправление. Удалите токены, коды доступа, ссылки подписки, IP и персональные данные. Не проверяйте чужие серверы и аккаунты и не останавливайте сервис ради демонстрации.

Владельцу нужны копии базы и окружения, ограниченные права администраторов/API-ключей, 2FA, закрытые служебные порты и проверка платёжных событий. Полная недоступность сервера требует внешнего мониторинга. См. [восстановление](docs/ru/backup-restore.md) и [диагностику](docs/ru/troubleshooting.md).
