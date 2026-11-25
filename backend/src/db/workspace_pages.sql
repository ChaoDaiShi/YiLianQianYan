CREATE TABLE pages (
  id TEXT PRIMARY KEY,
  workspace_id TEXT NOT NULL REFERENCES workspaces(id),
  parent_page_id TEXT REFERENCES pages(id),
  title TEXT NOT NULL,
  icon TEXT,
  cover_resource_id TEXT REFERENCES resources(id),
  sort_key INTEGER NOT NULL,
  favorite INTEGER NOT NULL DEFAULT 0,
  last_visited_at INTEGER,
  revision INTEGER NOT NULL CHECK (revision > 0),
  created_at INTEGER NOT NULL,
  updated_at INTEGER NOT NULL,
  deleted_at INTEGER,
  trash_root_id TEXT REFERENCES pages(id)
);
CREATE INDEX pages_tree ON pages(workspace_id, parent_page_id, deleted_at, sort_key, id);
CREATE INDEX pages_recent ON pages(workspace_id, deleted_at, last_visited_at DESC);
CREATE TABLE page_blocks (
  page_id TEXT NOT NULL REFERENCES pages(id),
  block_id TEXT NOT NULL,
  parent_block_id TEXT,
  block_type TEXT NOT NULL,
  content_json TEXT NOT NULL,
  sort_key INTEGER NOT NULL,
  revision INTEGER NOT NULL,
  created_at INTEGER NOT NULL,
  updated_at INTEGER NOT NULL,
  PRIMARY KEY (page_id, block_id),
  FOREIGN KEY (page_id, parent_block_id) REFERENCES page_blocks(page_id, block_id)
);
CREATE INDEX page_blocks_order ON page_blocks(page_id, sort_key);
CREATE TABLE page_revisions (
  page_id TEXT NOT NULL REFERENCES pages(id),
  revision INTEGER NOT NULL,
  source TEXT NOT NULL,
  metadata_json TEXT NOT NULL,
  blocks_json TEXT NOT NULL,
  created_at INTEGER NOT NULL,
  PRIMARY KEY (page_id, revision)
);
CREATE TABLE page_links (
  page_id TEXT NOT NULL,
  block_id TEXT NOT NULL,
  target_kind TEXT NOT NULL,
  target_id TEXT NOT NULL,
  FOREIGN KEY (page_id, block_id) REFERENCES page_blocks(page_id, block_id) ON DELETE CASCADE,
  PRIMARY KEY (page_id, block_id, target_kind, target_id)
);
CREATE INDEX page_links_target ON page_links(target_kind, target_id);
