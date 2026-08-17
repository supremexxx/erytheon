DO $$
BEGIN
    IF EXISTS (SELECT 1 FROM blue.ground_truth_observations LIMIT 1)
       OR EXISTS (SELECT 1 FROM blue.ground_truth_matches LIMIT 1)
       OR EXISTS (SELECT 1 FROM blue.ground_truth_refreshes LIMIT 1) THEN
        RAISE EXCEPTION 'refusing rollback: BLUE Ground Truth data exists';
    END IF;
END $$;

DROP TABLE blue.ground_truth_refreshes;
DROP TABLE blue.ground_truth_matches;
DROP TABLE blue.ground_truth_observations;
