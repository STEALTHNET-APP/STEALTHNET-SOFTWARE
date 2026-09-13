[All sections](../README.md) · [Русский](../../ru/sections/broadcasts.md) / [English](../../en/sections/broadcasts.md)

# Messaging your audience

Broadcasts send messages from your Telegram bot to a selected customer segment.

**Where to find it:** `#/broadcasts` — Sales.

## Workflow

1. Write the message.
2. choose the audience.
3. preview.
4. send yourself a test.
5. schedule or start delivery.

## Checking delivery

Test the text, links, buttons and audience segment before launch. Track deliveries and failures. If delivery stalls, check sn-worker and Telegram connectivity. Do not duplicate the campaign before inspecting its existing queue.

## Verify the result

Starting delivery sends messages to real people. Canceling stops the remaining queue but does not delete delivered messages. Telegram does not report read receipts to bots.

## Related guides

[First setup](getting-started.md) · [troubleshooting](../troubleshooting.md)

## Worker and delivery errors

The `sn-worker` service delivers campaigns in a separate loop. The panel shows its heartbeat and checks worker readiness and the configured bot token before starting. Results refresh every 10 seconds.

Invalid HTML, an unsupported button URL or invalid bot credentials stop the campaign with the actual error. Open it to inspect the cause and create a copy for corrected content. One failed campaign does not block others. Blocked bots and deleted chats count as recipient failures. Telegram's `retry_after` delays the next attempt for the requested period; transient failures are retried at most eight times per recipient.

**Send test** delivers the saved photo, text and button to the specified Telegram ID. The recipient must start your bot first. The test substitutes the administrator's username; plan and expiry fields use “—”. It does not start audience delivery.

Telegram has no idempotency key for `sendMessage` or `sendPhoto`: losing a response after delivery can lead to a duplicate on retry. Confirmed deliveries are not resent. Do not duplicate a running campaign while investigating it.

## Attaching a photo

Click **Attach photo** in a new broadcast and choose a JPG, PNG or WebP file from your device. The preview includes replace and remove controls. Click **Save draft** to persist changes. Duplicating a campaign retains its photo.

Send one photo with a caption and button, or a photo alone. Captions are limited to 1024 characters; messages without a photo allow 4096. The counter includes entered HTML markup; leave room for name, plan and expiry substitutions. Shorten an overlong caption or remove the photo. Test delivery uses the saved photo, caption and button; unsaved changes are not sent.

Files may be up to 10 MB and 16 MP, with combined width and height up to 10000 pixels and aspect ratio up to 20:1. The API validates and decodes the actual image, strips metadata and saves JPEG or PNG. Previews require an authenticated staff account with broadcast read access. Photos are stored in PostgreSQL, included in database backups and retained by `make update`; no separate directory or public URL is needed. Unattached uploads older than a day are cleaned up when the same operator next uploads a photo.

To verify, save and reopen the draft, check its photo, then send a test to your Telegram ID. Telegram should show the photo, caption and configured button in one message. Telegram limits: [sendPhoto](https://core.telegram.org/bots/api#sendphoto).
