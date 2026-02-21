-- PostgreSQL trigger for media change notifications.
-- The indexer will attempt to create this automatically on startup,
-- but you can run this manually if the database user lacks CREATE privileges.

CREATE OR REPLACE FUNCTION notify_media_changes() RETURNS trigger AS $$
BEGIN
    IF TG_OP = 'DELETE' THEN
        PERFORM pg_notify('media_changes', json_build_object('operation', TG_OP, 'id', OLD.id)::text);
        RETURN OLD;
    ELSE
        PERFORM pg_notify('media_changes', json_build_object('operation', TG_OP, 'id', NEW.id)::text);
        RETURN NEW;
    END IF;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER media_notify_trigger
    AFTER INSERT OR UPDATE OR DELETE ON media
    FOR EACH ROW EXECUTE FUNCTION notify_media_changes();
