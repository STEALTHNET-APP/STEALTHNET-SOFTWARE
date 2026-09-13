-- Durable notifications: committed events only, no backfill of historical customers/payments.
CREATE TABLE team_notifications (
 id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
 event_key text NOT NULL UNIQUE,
 kind text NOT NULL CHECK(kind IN ('payments','clients','tickets','service','test')),
 payload jsonb NOT NULL,
 chat_id text NOT NULL,
 thread_id bigint,
 status text NOT NULL DEFAULT 'pending' CHECK(status IN ('pending','sent','failed','canceled')),
 attempts integer NOT NULL DEFAULT 0,
 available_at timestamptz NOT NULL DEFAULT now(),
 created_at timestamptz NOT NULL DEFAULT now(),
 sent_at timestamptz,
 error text
);
CREATE INDEX team_notifications_pending ON team_notifications(available_at,id) WHERE status='pending';
CREATE TABLE service_incidents (
 key text PRIMARY KEY,
 problem text,
 revision bigint NOT NULL DEFAULT 1,
 changed_at timestamptz NOT NULL DEFAULT now()
);
CREATE FUNCTION enqueue_team_notification(category text, event text, data jsonb) RETURNS void LANGUAGE plpgsql AS $$
DECLARE target text; topic bigint;
BEGIN
 IF NOT COALESCE((SELECT value='true'::jsonb FROM settings WHERE key='bot.admin_alerts_enabled'),false)
    OR NOT COALESCE((SELECT value='true'::jsonb FROM settings WHERE key='bot.alert_'||category||'_enabled'),true) THEN RETURN; END IF;
 SELECT value#>>'{}' INTO target FROM settings WHERE key='bot.alert_chat_id';
 IF target IS NULL OR target !~ '^-[1-9][0-9]{0,18}$' THEN RETURN; END IF;
 BEGIN
  IF target::bigint>=0 THEN RETURN; END IF;
  SELECT NULLIF(value#>>'{}','')::bigint INTO topic FROM settings WHERE key='bot.alert_thread_id';
  IF topic<=0 THEN RETURN; END IF;
 EXCEPTION WHEN invalid_text_representation OR numeric_value_out_of_range THEN RETURN;
 END;
 INSERT INTO team_notifications(event_key,kind,payload,chat_id,thread_id)
 VALUES(event,category,data,target,topic) ON CONFLICT(event_key) DO NOTHING;
END $$;
CREATE FUNCTION notify_team_event() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
 IF TG_TABLE_NAME='clients' THEN
  PERFORM enqueue_team_notification('clients','client:'||NEW.id,jsonb_build_object('client_id',NEW.id));
 ELSIF TG_TABLE_NAME='payments' THEN
  IF TG_OP='UPDATE' AND OLD.status=NEW.status THEN RETURN NEW; END IF;
  IF NEW.status IN ('success','refunded') THEN
   PERFORM enqueue_team_notification('payments','payment:'||NEW.id||':'||NEW.status,jsonb_build_object('payment_id',NEW.id,'status',NEW.status));
  END IF;
 ELSIF TG_TABLE_NAME='tickets' THEN
  PERFORM enqueue_team_notification('tickets','ticket:'||NEW.id,jsonb_build_object('ticket_id',NEW.id,'reply',false));
 ELSIF TG_TABLE_NAME='ticket_messages' AND NEW.author_kind='client' THEN
  IF EXISTS(SELECT 1 FROM ticket_messages WHERE ticket_id=NEW.ticket_id AND id<>NEW.id) THEN
   PERFORM enqueue_team_notification('tickets','ticket_message:'||NEW.id,jsonb_build_object('ticket_id',NEW.ticket_id,'message_id',NEW.id,'reply',true));
  END IF;
 END IF;
 RETURN NEW;
END $$;
CREATE TRIGGER clients_notify_team AFTER INSERT ON clients FOR EACH ROW EXECUTE FUNCTION notify_team_event();
CREATE TRIGGER payments_notify_team AFTER INSERT OR UPDATE OF status ON payments FOR EACH ROW EXECUTE FUNCTION notify_team_event();
CREATE TRIGGER tickets_notify_team AFTER INSERT ON tickets FOR EACH ROW EXECUTE FUNCTION notify_team_event();
CREATE TRIGGER ticket_messages_notify_team AFTER INSERT ON ticket_messages FOR EACH ROW EXECUTE FUNCTION notify_team_event();
