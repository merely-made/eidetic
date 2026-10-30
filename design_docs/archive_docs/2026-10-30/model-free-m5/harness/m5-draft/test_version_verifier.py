#!/usr/bin/env python3
"""Check old/current evidence joins without creating retained source records."""
import copy
import unittest
from verify_receipt import verify_old_and_current

class VersionTest(unittest.TestCase):
    def test_exact_versions_are_joined_to_owner_and_chronology(self):
        def row(label, manifest, text_hash, when):
            key = dict(session='s', graph='g', node='n', annotation_manifest=manifest)
            content = dict(text='M5 ' + label + ' archive', text_hash=text_hash,
                           captured_at_ms=when, raw_manifest='raw-' + label)
            return {'phase':'Ready', 'target':{'key':key, 'text_hash':text_hash}, 'content':content}
        a, b = row('A1', 'a', 'h1', 1), row('A2', 'b', 'h2', 2)
        old, current = {'readers':[a]}, {'readers':[b]}
        owner = {'captures':[{'admission':{'key':x['target']['key']}, 'content':x['content']} for x in (a,b)]}
        verify_old_and_current(old, current, owner)
        wrong = copy.deepcopy(current); wrong['readers'][0]['target']['text_hash'] = 'h1'
        with self.assertRaises(ValueError): verify_old_and_current(old, wrong, owner)
        wrong = copy.deepcopy(owner); wrong['captures'].pop()
        with self.assertRaises(ValueError): verify_old_and_current(old, current, wrong)
        wrong = copy.deepcopy(current); wrong['readers'][0]['target']['key']['node'] = 'other'
        with self.assertRaises(ValueError): verify_old_and_current(old, wrong, owner)
        wrong = copy.deepcopy(owner); wrong['captures'][1]['content']['captured_at_ms'] = 1
        current2 = copy.deepcopy(current); current2['readers'][0]['content']['captured_at_ms'] = 1
        with self.assertRaises(ValueError): verify_old_and_current(old, current2, wrong)

if __name__ == '__main__': unittest.main()
