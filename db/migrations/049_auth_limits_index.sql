-- Индекс по времени окна для быстрой периодической очистки лимитов кабинета воркером
CREATE INDEX IF NOT EXISTS cabinet_auth_limits_window_idx ON cabinet_auth_limits(window_start);
