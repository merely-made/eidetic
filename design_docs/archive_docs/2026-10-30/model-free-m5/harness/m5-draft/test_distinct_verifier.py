#!/usr/bin/env python3
"""Check rejection of falsely labelled comparison rows without retained data."""
import unittest
from verify_receipt import verify_distinct_comparison, verify_disagreement

class DistinctTest(unittest.TestCase):
    def test_distinct_evidence_and_contradictions(self):
        base = dict(entity='e', property='p', schema_revision='r', context='c',
                    status='active', valid_time={'start': 0, 'end': 2}, value={'Integer': 1})
        single = [{'id': 'p', 'revision': 'r', 'cardinality': 'Single'}]
        multi = [{'id': 'p', 'revision': 'r', 'cardinality': 'Multiple'}]
        with self.assertRaises(ValueError): verify_distinct_comparison(base, dict(base), single)
        with self.assertRaises(ValueError): verify_distinct_comparison(base, dict(base, value={'Integer': 2}), single)
        with self.assertRaises(ValueError): verify_distinct_comparison(base, dict(base, status=None), multi)
        with self.assertRaises(ValueError): verify_distinct_comparison(base, dict(base, entity='other'), [])
        verify_distinct_comparison(base, dict(base, entity='other'), single)
        verify_distinct_comparison(base, dict(base, valid_time={'start': 2, 'end': 3}), single)
        verify_distinct_comparison(base, dict(base, value={'Integer': 2}), multi)
        other = dict(base, value={'Integer': 2})
        verify_disagreement(base, other, single)
        with self.assertRaises(ValueError): verify_disagreement(base, other, multi)
        with self.assertRaises(ValueError): verify_disagreement(base, dict(base), single)

if __name__ == '__main__': unittest.main()
