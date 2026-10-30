# Retained reader admission scan hoist

Approved. The only delta moves the unchanged admission enumeration before the live-reader loop. The loop does not mutate app graph admission; each reader still independently checks exact target, session, current identity, custody epoch, graph address and selection. This removes repeated graph scans without changing the admitted results. All 39 hashes in m5-cold-reader-hoisted-hashes.json match the frozen source. The full host gate is running against that source; native cold reopen remains pending.
