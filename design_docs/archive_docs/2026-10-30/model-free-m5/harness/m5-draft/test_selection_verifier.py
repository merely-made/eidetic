#!/usr/bin/env python3
"""Independent selection-byte checks against synthetic local test files."""
import json, tempfile, unittest
from pathlib import Path
from verify_receipt import verify_copied_selection

class SelectionVerifier(unittest.TestCase):
    def test_private_source_and_package_ranges(self):
        with tempfile.TemporaryDirectory() as td:
            base = Path(td); copied = base/'inspection'/'notes'/'a.txt'; copied.parent.mkdir(parents=True)
            text = 'M5 private preface zebra 🐦 only this linked finding M5 private suffix otter'
            copied.write_text(text)
            quote = 'only this linked finding'; raw = text.encode(); selected = quote.encode()
            byte_start = raw.index(selected); scalar_start = text.index(quote)
            package = {'payloads':[{'bytes':list(selected)}]}
            (base/'package.json').write_text(json.dumps(package))
            selection = {'source_relative_path':'notes/a.txt', 'source_hash':'blake3-fact',
                         'admission':{'id':'admitted'}, 'clip':{'id':'clip'}, 'package':package,
                         'quote':quote, 'byte_start':byte_start, 'byte_end':byte_start+len(selected),
                         'scalar_start':scalar_start, 'scalar_end':scalar_start+len(quote)}
            evidence = {'inspection_root':'inspection', 'copied_source':'inspection/notes/a.txt',
                        'exported_package':'package.json'}
            files = {'inspection/notes/a.txt':'bound', 'package.json':'bound'}
            verify_copied_selection(base, files, evidence, {'selection':selection})
            selection['byte_end'] += 1
            with self.assertRaises(ValueError): verify_copied_selection(base, files, evidence, {'selection':selection})
            selection['byte_end'] -= 1; selection['source_relative_path'] = '../a.txt'
            with self.assertRaises(ValueError): verify_copied_selection(base, files, evidence, {'selection':selection})

if __name__ == '__main__': unittest.main()
