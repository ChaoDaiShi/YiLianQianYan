CREATE VIRTUAL TABLE memory_fts USING fts5(
    content, content='memories', content_rowid='rowid', tokenize='unicode61 remove_diacritics 2'
);
INSERT INTO memory_fts(memory_fts) VALUES ('rebuild');

CREATE TRIGGER memory_fts_on_insert AFTER INSERT ON memories BEGIN
    INSERT INTO memory_fts(rowid,content) VALUES (NEW.rowid,NEW.content);
END;
CREATE TRIGGER memory_fts_on_update AFTER UPDATE OF content ON memories BEGIN
    INSERT INTO memory_fts(memory_fts,rowid,content) VALUES ('delete',OLD.rowid,OLD.content);
    INSERT INTO memory_fts(rowid,content) VALUES (NEW.rowid,NEW.content);
END;
CREATE TRIGGER memory_fts_on_delete AFTER DELETE ON memories BEGIN
    INSERT INTO memory_fts(memory_fts,rowid,content) VALUES ('delete',OLD.rowid,OLD.content);
END;
