-- Only stable resource references and transition metadata belong in this index.
CREATE TABLE activity_events (
    sequence INTEGER PRIMARY KEY AUTOINCREMENT,
    event_id TEXT UNIQUE NOT NULL DEFAULT (lower(hex(randomblob(16)))),
    stream_id TEXT NOT NULL,
    event_type TEXT NOT NULL,
    subject_id TEXT NOT NULL,
    payload TEXT NOT NULL,
    created_at INTEGER NOT NULL DEFAULT (CAST((julianday('now') - 2440587.5)*86400000 AS INTEGER))
);
CREATE INDEX activity_stream_cursor ON activity_events(stream_id,sequence);
CREATE TRIGGER activity_message_insert AFTER INSERT ON messages BEGIN
    INSERT INTO activity_events(stream_id,event_type,subject_id,payload)
    VALUES('conversation:'||NEW.conversation_id,'message.created',NEW.id,json_object('role',NEW.role));
END;
CREATE TRIGGER activity_turn_insert AFTER INSERT ON conversation_turns BEGIN
    INSERT INTO activity_events(stream_id,event_type,subject_id,payload)
    VALUES('conversation:'||NEW.conversation_id,'turn.started',NEW.id,json_object('status',NEW.status));
END;
CREATE TRIGGER activity_turn_change AFTER UPDATE OF status ON conversation_turns
WHEN NEW.status IS NOT OLD.status BEGIN
    INSERT INTO activity_events(stream_id,event_type,subject_id,payload)
    VALUES('conversation:'||NEW.conversation_id,'turn.status',NEW.id,json_object('status',NEW.status));
END;
CREATE TRIGGER activity_task_insert AFTER INSERT ON tasks BEGIN
    INSERT INTO activity_events(stream_id,event_type,subject_id,payload)
    VALUES('task:'||NEW.id,'task.created',NEW.id,json_object('status',NEW.status));
END;
CREATE TRIGGER activity_task_status AFTER UPDATE OF status ON tasks
WHEN NEW.status IS NOT OLD.status BEGIN
    INSERT INTO activity_events(stream_id,event_type,subject_id,payload)
    VALUES('task:'||NEW.id,'task.status',NEW.id,json_object('status',NEW.status));
END;
CREATE TRIGGER activity_page_revision AFTER INSERT ON page_revisions BEGIN
    INSERT INTO activity_events(stream_id,event_type,subject_id,payload)
    VALUES('page:'||NEW.page_id,'page.revised',NEW.page_id,json_object('revision',NEW.revision));
END;
CREATE TRIGGER activity_memory_insert AFTER INSERT ON memories BEGIN
    INSERT INTO activity_events(stream_id,event_type,subject_id,payload)
    VALUES('memory:'||NEW.id,'memory.created',NEW.id,'{}');
END;
CREATE TRIGGER activity_memory_edit AFTER UPDATE OF content ON memories
WHEN NEW.content IS NOT OLD.content BEGIN
    INSERT INTO activity_events(stream_id,event_type,subject_id,payload)
    VALUES('memory:'||NEW.id,'memory.revised',NEW.id,'{}');
END;
CREATE TRIGGER activity_memory_delete AFTER DELETE ON memories BEGIN
    INSERT INTO activity_events(stream_id,event_type,subject_id,payload)
    VALUES('memory:'||OLD.id,'memory.removed',OLD.id,'{}');
END;
CREATE TRIGGER activity_resource_insert AFTER INSERT ON resources BEGIN
    INSERT INTO activity_events(stream_id,event_type,subject_id,payload)
    VALUES('asset:'||NEW.id,'asset.created',NEW.id,'{}');
END;
CREATE TRIGGER activity_profile_edit AFTER UPDATE OF profile ON local_onboarding
WHEN NEW.profile IS NOT OLD.profile BEGIN
    INSERT INTO activity_events(stream_id,event_type,subject_id,payload)
    VALUES('profile:'||NEW.scope,'profile.updated',NEW.scope,json_object('revision',NEW.revision));
END;
CREATE TRIGGER activity_rich_create AFTER INSERT ON message_component_states BEGIN
    INSERT INTO activity_events(stream_id,event_type,subject_id,payload)
    VALUES('conversation:'||(SELECT conversation_id FROM messages WHERE id=NEW.message_id),'component.submitted',NEW.message_id,json_object('component_id',NEW.component_id,'revision',NEW.state_version));
END;
CREATE TRIGGER activity_rich_update AFTER UPDATE OF state_version ON message_component_states
WHEN NEW.state_version IS NOT OLD.state_version BEGIN
    INSERT INTO activity_events(stream_id,event_type,subject_id,payload)
    VALUES('conversation:'||(SELECT conversation_id FROM messages WHERE id=NEW.message_id),'component.submitted',NEW.message_id,json_object('component_id',NEW.component_id,'revision',NEW.state_version));
END;
