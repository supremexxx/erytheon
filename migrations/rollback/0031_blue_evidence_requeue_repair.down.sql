-- Migration 0031 only repairs mutable retry cursors. Rolling it back must not
-- rewind counters because the corresponding evidence runs are append-only.
SELECT 1;
