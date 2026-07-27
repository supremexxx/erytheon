DO $$
BEGIN
    IF EXISTS (SELECT 1 FROM features.historical_calendar_days)
       OR EXISTS (SELECT 1 FROM features.calendar_rule_versions) THEN
        RAISE EXCEPTION
            'refusing destructive rollback: historical calendar data exists';
    END IF;
END
$$;

DROP TABLE features.historical_calendar_days;
DROP TABLE features.calendar_rule_versions;
