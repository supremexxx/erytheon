DO $$
BEGIN
    IF EXISTS (SELECT 1 FROM validation.event_label_quality)
       OR EXISTS (SELECT 1 FROM validation.event_geographic_quality)
       OR EXISTS (SELECT 1 FROM validation.event_combustibility_assessments)
       OR EXISTS (SELECT 1 FROM validation.combustible_cell_candidates)
       OR EXISTS (SELECT 1 FROM validation.duplicate_candidate_pairs)
       OR EXISTS (SELECT 1 FROM validation.duplicate_candidate_groups)
       OR EXISTS (SELECT 1 FROM validation.duplicate_candidate_members)
       OR EXISTS (SELECT 1 FROM validation.coordinate_groups)
       OR EXISTS (SELECT 1 FROM validation.rule_versions) THEN
        RAISE EXCEPTION
            'refusing destructive rollback: validation quality data exists';
    END IF;
END
$$;

DROP TABLE validation.duplicate_candidate_members;
DROP TABLE validation.duplicate_candidate_groups;
DROP TABLE validation.duplicate_candidate_pairs;
DROP TABLE validation.combustible_cell_candidates;
DROP TABLE validation.event_combustibility_assessments;
DROP TABLE validation.event_geographic_quality;
DROP TABLE validation.event_label_quality;
DROP TABLE validation.coordinate_groups;
DROP TABLE validation.rule_versions;
DROP SCHEMA validation;
