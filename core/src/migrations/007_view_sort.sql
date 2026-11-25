-- Sorting in list and tag views (2026-11-25): each list and tag keeps its sort, a
-- JSON array of sort keys like `filter.sort`. Empty means the default (due, priority).
-- The Inbox's sort is per device, in `meta` (key `inbox_sort`).
ALTER TABLE list ADD COLUMN sort TEXT NOT NULL DEFAULT '[]';
ALTER TABLE tag ADD COLUMN sort TEXT NOT NULL DEFAULT '[]';
