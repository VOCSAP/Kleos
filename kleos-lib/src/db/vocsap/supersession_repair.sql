-- One-shot repair of the supersedes links written before the dedup keeper
-- rule of Patch 60. Runs inside a SAVEPOINT opened by the caller, together
-- with the archive trigger and the one-shot marker.
--
-- Holding keeper, as in the archive trigger: is_archived = 0 OR is_forgotten = 1.

-- R1: a retired memory whose keepers are all archived (and not forgotten) is
-- served again. A retired memory without any link is left alone: nothing says
-- why it was retired.
UPDATE memories SET is_superseded = 0
WHERE is_superseded = 1
  AND id IN (
      SELECT l.target_id FROM memory_links l
      WHERE l.type = 'supersedes'
        AND NOT EXISTS (
            SELECT 1 FROM memory_links h
            JOIN memories k ON k.id = h.source_id
            WHERE h.target_id = l.target_id AND h.type = 'supersedes'
              AND (k.is_archived = 0 OR k.is_forgotten = 1)
        )
  );

-- R2: reverse the links where the retired memory wins under the keeper rule
-- (higher importance, then higher id). Both sides must be served versions, and
-- a manual correction (reconsolidations row carrying the keeper text) is a
-- deliberate replacement, never reversed. The set is frozen before any write
-- so a swap cannot change which other links qualify. CROSS JOIN pins the
-- loop order on the links: left free, the planner drives from the two
-- is_forgotten index scans and pairs every memory with every other one.
DROP TABLE IF EXISTS temp.vocsap_supersession_flip;
CREATE TEMP TABLE vocsap_supersession_flip AS
SELECT l.id AS link_id, l.source_id AS loser, l.target_id AS winner
FROM memory_links l
CROSS JOIN memories k ON k.id = l.source_id
CROSS JOIN memories t ON t.id = l.target_id
WHERE l.type = 'supersedes'
  AND t.is_superseded = 1
  AND k.is_archived = 0 AND k.is_forgotten = 0 AND k.is_latest = 1 AND k.status <> 'pending'
  AND t.is_archived = 0 AND t.is_forgotten = 0 AND t.is_latest = 1 AND t.status <> 'pending'
  AND (t.importance > k.importance OR (t.importance = k.importance AND t.id > k.id))
  AND NOT EXISTS (
      SELECT 1 FROM reconsolidations r
      WHERE r.memory_id = l.target_id AND r.new_content = k.content
  );

-- A reverse link that already exists would collide on UNIQUE(source_id,
-- target_id, type) at boot: drop the forward link instead of swapping it.
DELETE FROM memory_links
WHERE id IN (
    SELECT f.link_id FROM vocsap_supersession_flip f
    WHERE EXISTS (
        SELECT 1 FROM memory_links r
        WHERE r.source_id = f.winner AND r.target_id = f.loser AND r.type = 'supersedes'
    )
);

UPDATE memory_links SET source_id = target_id, target_id = source_id
WHERE id IN (SELECT link_id FROM vocsap_supersession_flip);

UPDATE memories SET is_superseded = 1
WHERE id IN (SELECT loser FROM vocsap_supersession_flip);

-- A winner that is still the target of a holding keeper (a chain) stays hidden.
UPDATE memories SET is_superseded = 0
WHERE id IN (
    SELECT f.winner FROM vocsap_supersession_flip f
    WHERE NOT EXISTS (
        SELECT 1 FROM memory_links h
        JOIN memories k ON k.id = h.source_id
        WHERE h.target_id = f.winner AND h.type = 'supersedes'
          AND (k.is_archived = 0 OR k.is_forgotten = 1)
    )
);

DROP TABLE temp.vocsap_supersession_flip;

CREATE TABLE IF NOT EXISTS vocsap_one_shot (
    name TEXT PRIMARY KEY,
    applied_at TEXT NOT NULL DEFAULT (datetime('now'))
);
INSERT OR IGNORE INTO vocsap_one_shot (name) VALUES ('supersession_repair');
