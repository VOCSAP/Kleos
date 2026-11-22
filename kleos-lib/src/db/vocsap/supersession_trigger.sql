-- Archiving a keeper gives its retired targets back, unless another keeper
-- still holds them. A holding keeper is one that is not archived, or one that
-- is forgotten: forgetting a keeper keeps its targets hidden, archiving does
-- not. AFTER UPDATE sees the archived row, so the keeper being archived drops
-- out of the holding test without an explicit exclusion. Links are kept:
-- un-archiving the keeper does not hide the target again.
CREATE TRIGGER IF NOT EXISTS vocsap_supersession_release_on_archive
AFTER UPDATE OF is_archived ON memories
WHEN NEW.is_archived = 1 AND OLD.is_archived = 0 AND NEW.is_forgotten = 0
BEGIN
    UPDATE memories SET is_superseded = 0
    WHERE is_superseded = 1
      AND id IN (
          SELECT l.target_id FROM memory_links l
          WHERE l.source_id = NEW.id AND l.type = 'supersedes'
            AND NOT EXISTS (
                SELECT 1 FROM memory_links h
                JOIN memories k ON k.id = h.source_id
                WHERE h.target_id = l.target_id AND h.type = 'supersedes'
                  AND (k.is_archived = 0 OR k.is_forgotten = 1)
            )
      );
END;
