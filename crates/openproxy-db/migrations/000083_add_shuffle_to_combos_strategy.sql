-- 000083_add_shuffle_to_combos_strategy.sql
-- Add 'shuffle' to combos.strategy CHECK constraint.

PRAGMA foreign_keys = OFF;

CREATE TABLE combos_new (
  id                     INTEGER PRIMARY KEY AUTOINCREMENT,
  name                   TEXT NOT NULL UNIQUE,
  strategy               TEXT NOT NULL,
  race_size              INTEGER NOT NULL DEFAULT 1,
  created_at             TEXT NOT NULL DEFAULT (datetime('now')),
  context_window         INTEGER,
  priority_mode          TEXT,
  cooldown_mode          TEXT,
  cooldown_base_secs     INTEGER,
  cooldown_max_secs      INTEGER,
  cooldown_factor        INTEGER,
  lkgp_exploration_rate  REAL,
  selection_window_secs  INTEGER,
  preventive_rate_limit  INTEGER NOT NULL DEFAULT 0,
  decision_model         TEXT,
  decision_timeout_ms    INTEGER DEFAULT 100,
  CHECK (strategy IN ('priority', 'round_robin', 'shuffle')),
  CHECK (race_size >= 1 AND race_size <= 8)
);

INSERT INTO combos_new (
  id, name, strategy, race_size, created_at, context_window,
  priority_mode, cooldown_mode, cooldown_base_secs, cooldown_max_secs,
  cooldown_factor, lkgp_exploration_rate, selection_window_secs,
  preventive_rate_limit, decision_model, decision_timeout_ms
)
SELECT
  id, name, strategy, race_size, created_at, context_window,
  priority_mode, cooldown_mode, cooldown_base_secs, cooldown_max_secs,
  cooldown_factor, lkgp_exploration_rate, selection_window_secs,
  preventive_rate_limit, decision_model, decision_timeout_ms
FROM combos;

DROP TABLE combos;
ALTER TABLE combos_new RENAME TO combos;

PRAGMA foreign_keys = ON;
PRAGMA foreign_key_check;
