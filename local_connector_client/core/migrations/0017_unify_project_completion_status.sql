UPDATE project_requirements
SET status = 'done'
WHERE LOWER(TRIM(status)) IN ('completed', 'succeeded', 'success');

UPDATE project_work_items
SET status = 'done'
WHERE LOWER(TRIM(status)) IN ('completed', 'succeeded', 'success');
