[All guides](README.md) · [Русский](../ru/team-notifications.md) / [English](../en/team-notifications.md)

# Team notifications in Telegram

Open **Telegram bot → Team notifications**.

1. Add the service bot to the staff group and allow it to send messages.
2. Run `/chatid` in the intended topic. Use `/chatid@your_bot_name` if needed. The bot returns group and topic IDs.
3. Enter the negative group ID and optional positive topic ID. An empty topic means the general chat.
4. Enable alerts, choose categories and save.
5. Send a test. Inspect group ID, topic and bot permissions if delivery fails.

## Categories

- Payments/refunds: successful plans, add-ons and refunds; unpaid invoices do not generate payment alerts.
- New customers: newly created customer records.
- Tickets: new tickets and subsequent customer replies; staff replies are not rebroadcast.
- Service problems: missing node reports, engine failures, high CPU/RAM and worker failures. Recovery sends a separate event.

Node report timeout is three minutes; new nodes receive ten minutes for initial setup. CPU/RAM alert thresholds are 95%, with recovery below 85%. Disabled nodes and disabled node alerts are excluded. Identical ongoing problems are not sent every cycle. A complete panel/database/Telegram outage still requires external monitoring.

## Delivery and retries

Events enter the database with their business action. sn-worker checks the queue every ten seconds using the main bot token. Temporary Telegram errors retry with increasing delays, up to eight attempts. Invalid destination/permission/token errors appear as failed deliveries. Retry failed returns recent errors for the current destination to the queue.

Disabling a category or changing the destination cancels its old pending events. Completed attempt history is kept for 30 days. A process crash after Telegram accepts a message but before local acknowledgement can cause a repeat delivery.

Messages include a short event summary, object identifier and panel link. Full conversations, subscription links, VPN keys and payment secrets are not included. Test and production messages are sent only after the owner configures the destination.
