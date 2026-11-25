-- Keep the historical table's column order so the previous application binary
-- can still open the profile and use its original INSERT statements.
CREATE TABLE memory_version_details (
    memory_id TEXT NOT NULL,
    revision INTEGER NOT NULL,
    relationship_json TEXT,
    relationship_known INTEGER NOT NULL,
    baseline INTEGER NOT NULL,
    conversation_id TEXT,
    message_id TEXT,
    graph_id TEXT,
    report_id TEXT,
    captured_with_revision INTEGER NOT NULL,
    PRIMARY KEY(memory_id, revision),
    FOREIGN KEY(memory_id, revision) REFERENCES memory_versions(memory_id, revision) ON DELETE CASCADE
);

CREATE VIEW memory_live_version_metadata AS
SELECT m.id,
       CASE WHEN json_valid(m.metadata) THEN m.metadata END AS metadata,
       COALESCE(m.source_conversation_id, removed.conversation_id) AS conversation_id
FROM memories m LEFT JOIN removed_memory_sources removed ON removed.memory_id=m.id;

-- Only the still-current row has verifiable live relationship metadata at
-- migration. Never copy today's relation into genuine historical revisions.
INSERT INTO memory_version_details
SELECT v.memory_id, v.revision, json_extract(m.metadata,'$.relationship'),
       CASE WHEN m.metadata IS NOT NULL THEN 1
            WHEN v.source='cloud_sync' THEN 0 ELSE 1 END,
       v.reason='legacy import; real-world validity unknown',
       m.conversation_id, json_extract(m.metadata,'$.source_message_id'),
       json_extract(m.metadata,'$.source_graph_id'), json_extract(m.metadata,'$.source_report_id'), 0
FROM memory_versions v JOIN memory_governance g ON g.memory_id=v.memory_id AND g.revision=v.revision
JOIN memory_live_version_metadata m ON m.id=v.memory_id;

CREATE TRIGGER memory_version_capture_details AFTER INSERT ON memory_versions BEGIN
    INSERT INTO memory_version_details
    SELECT NEW.memory_id, NEW.revision, json_extract(m.metadata,'$.relationship'), 1,
           NEW.reason='legacy import; real-world validity unknown',
           m.conversation_id, json_extract(m.metadata,'$.source_message_id'),
           json_extract(m.metadata,'$.source_graph_id'), json_extract(m.metadata,'$.source_report_id'), 1
    FROM memory_live_version_metadata m WHERE m.id=NEW.memory_id;
END;

-- Some legacy cleanup paths used deletes that didn't enable SQLite foreign keys.
CREATE TRIGGER memory_version_clear_details AFTER DELETE ON memory_versions BEGIN
    DELETE FROM memory_version_details WHERE memory_id=OLD.memory_id AND revision=OLD.revision;
END;

DROP TRIGGER memory_governance_changed;
CREATE TRIGGER memory_governance_changed AFTER UPDATE OF content,category,metadata,source,source_conversation_id ON memories
WHEN NEW.content IS NOT OLD.content OR NEW.category IS NOT OLD.category OR NEW.metadata IS NOT OLD.metadata
     OR NEW.source IS NOT OLD.source
     OR (NEW.source_conversation_id IS NOT OLD.source_conversation_id
         AND NOT (NEW.source_conversation_id IS NULL
                  AND EXISTS (SELECT 1 FROM removed_memory_sources r
                              WHERE r.memory_id=NEW.id AND r.conversation_id=OLD.source_conversation_id)))
BEGIN
    UPDATE memory_governance SET revision=revision+1,
        embedding_revision=CASE WHEN NEW.content IS OLD.content AND embedding_revision=revision THEN revision+1 ELSE NULL END,
        embedding_hash=CASE WHEN NEW.content IS OLD.content THEN embedding_hash ELSE NULL END,
        embedding_model=CASE WHEN NEW.content IS OLD.content THEN embedding_model ELSE NULL END,
        embedding_dimension=CASE WHEN NEW.content IS OLD.content THEN embedding_dimension ELSE NULL END,
        embedding_generation=CASE WHEN NEW.content IS OLD.content THEN embedding_generation ELSE NULL END
    WHERE memory_id=NEW.id;
    UPDATE memories SET embedding=NULL WHERE id=NEW.id AND NEW.content IS NOT OLD.content;
    INSERT INTO memory_versions
    SELECT NEW.id,revision,NEW.content,NEW.category,NEW.source,source_ref,scope,status,reason,NEW.updated_at,valid_from,valid_until
    FROM memory_governance WHERE memory_id=NEW.id;
END;
