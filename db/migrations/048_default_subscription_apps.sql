-- A production installation must have the connection apps without loading demo data.
-- Add missing platform/name pairs once; keep the owner's URLs, order and visibility.
INSERT INTO subscription_page_apps (platform, sort_order, name, deeplink, store_url)
SELECT v.platform, v.sort_order, v.name, v.deeplink, v.store_url
FROM (VALUES
    ('ios', 10, 'Happ', 'happ://add/{{URL}}', 'https://happ.info/'),
    ('ios', 20, 'Streisand', 'streisand://import/{{URL}}', 'https://apps.apple.com/us/app/streisand/id6450534064'),
    ('ios', 30, 'V2Box', 'v2box://install-sub?url={{URL}}', 'https://apps.apple.com/us/app/v2box-v2ray-client/id6446814690'),
    ('ios', 40, 'Shadowrocket', NULL, 'https://apps.apple.com/us/app/shadowrocket/id932747118'),
    ('android', 10, 'Happ', 'happ://add/{{URL}}', 'https://happ.info/'),
    ('android', 20, 'v2rayNG', 'v2rayng://install-sub?url={{URL}}', 'https://github.com/2dust/v2rayNG/releases/latest'),
    ('android', 30, 'FlClash', 'clash://install-config?url={{URL}}', 'https://github.com/chen08209/FlClash/releases/latest'),
    ('android', 40, 'NekoBox', NULL, 'https://github.com/MatsuriDayo/NekoBoxForAndroid/releases/latest'),
    ('windows', 10, 'Hiddify', 'hiddify://import/{{URL}}', 'https://github.com/hiddify/hiddify-app/releases/latest'),
    ('windows', 20, 'v2rayN', NULL, 'https://github.com/2dust/v2rayN/releases/latest'),
    ('windows', 30, 'Karing', 'karing://install-config?url={{URL}}', 'https://karing.app/en/download'),
    ('macos', 10, 'Happ', 'happ://add/{{URL}}', 'https://happ.info/'),
    ('macos', 20, 'Hiddify', 'hiddify://import/{{URL}}', 'https://github.com/hiddify/hiddify-app/releases/latest'),
    ('macos', 30, 'Stash', 'stash://install-config?url={{URL}}', 'https://stash.ws/download'),
    ('linux', 10, 'Hiddify', 'hiddify://import/{{URL}}', 'https://github.com/hiddify/hiddify-app/releases/latest'),
    ('linux', 20, 'sing-box', NULL, 'https://sing-box.sagernet.org/installation/package-manager/')
) AS v(platform, sort_order, name, deeplink, store_url)
WHERE NOT EXISTS (
    SELECT 1 FROM subscription_page_apps a
    WHERE a.platform=v.platform AND lower(btrim(a.name))=lower(v.name)
);
