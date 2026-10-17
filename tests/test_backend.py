import copy
import json
import unittest
from unittest.mock import patch
from types import SimpleNamespace

import backend_api
import generate_batches_backend as generator
import sync_service


def pr(number, files=None, **extra):
    return {'number': number, 'files': files or [f'{number}.cpp'],
            'github_state': 'open', 'category': 'Core', 'total_lines': 10,
            'assigned_reviewer': 'none', **extra}


class BackendTests(unittest.TestCase):
    def test_claimed_batch_membership_is_excluded_without_mutation(self):
        rows = [pr(1), pr(2), pr(3)]
        existing = [{'id': 'legacy', 'status': 'claimed', 'claimed_by': 'owner',
                     'pr_numbers': [1, 2]}]
        before = copy.deepcopy(existing)
        batches = generator.generate_batches(rows, existing)
        self.assertEqual([b['pr_numbers'] for b in batches], [[3]])
        self.assertEqual(existing, before)

    def test_draft_closed_unknown_and_legacy_assignment_are_excluded(self):
        rows = [pr(1, is_draft=True), pr(2, github_state='merged'),
                pr(3, github_state='unknown'), pr(4, assigned_reviewer='legacy'), pr(5)]
        self.assertEqual(generator.generate_batches(rows, [])[0]['pr_numbers'], [5])

    def test_overlapping_files_never_share_a_batch(self):
        batches = generator.generate_batches([pr(1, ['shared']), pr(2, ['shared']), pr(3)], [])
        for batch in batches:
            self.assertFalse({1, 2}.issubset(batch['pr_numbers']))
        self.assertEqual(sorted(n for b in batches for n in b['pr_numbers']), [1, 2, 3])

    def test_membership_ids_are_stable_across_input_order(self):
        rows = [pr(3), pr(1), pr(2)]
        self.assertEqual(generator.generate_batches(rows, []), generator.generate_batches(rows[::-1], []))

    def test_pagination_continues_after_server_capped_short_page(self):
        with patch.object(backend_api, 'request', side_effect=[[{'number': 1}], [{'number': 2}], []]) as req:
            self.assertEqual(len(backend_api.read_all('prs?order=number.asc')), 2)
            self.assertIn('offset=2', req.call_args.args[0])

    def test_failed_fetch_never_writes(self):
        with patch.object(sync_service, 'fetch_github_prs', side_effect=RuntimeError('fetch failed')):
            with patch.object(sync_service, 'request') as request:
                with self.assertRaises(RuntimeError):
                    sync_service.sync()
                request.assert_not_called()

    def test_empty_open_set_still_reconciles_closed_states(self):
        states = [{'number': 1, 'github_state': 'merged'}]
        with patch.object(sync_service, 'fetch_github_prs', return_value=([], states)):
            with patch.object(sync_service, 'cleanup_non_open_prs') as cleanup:
                with patch.object(sync_service, 'request') as request:
                    sync_service.sync()
                    self.assertEqual(request.call_args.args, ('rpc/sync_github_snapshot',
                        {'p_records': [], 'p_states': states}))
                    cleanup.assert_called_once()

    def test_incomplete_github_files_abort(self):
        detail = {'state': 'open', 'changed_files': 2}
        with patch.object(sync_service, 'github_pages', side_effect=[
                [{'number': 1, 'state': 'open'}], [{'filename': 'one.cpp'}]]):
            with patch.object(sync_service.subprocess, 'run', return_value=SimpleNamespace(
                    returncode=0, stdout=json.dumps(detail))):
                with self.assertRaisesRegex(RuntimeError, 'Incomplete file list'):
                    sync_service.fetch_github_prs()

    def test_smart_mode_cohesive_cluster_and_singleton_filtering(self):
        rows = [
            pr(1, ['libc/src/Math.cpp']),
            pr(2, ['libc/src/Math.cpp']),
            pr(3, ['libc/src/Math.cpp', 'CMakeLists.txt']),
            pr(4, ['isolated_singleton.cpp']),
        ]
        batches = generator.generate_batches(rows, [], smart_mode=True)
        self.assertEqual(len(batches), 1)
        self.assertEqual(batches[0]['batch_type'], 'cohesive_cluster')
        self.assertEqual(batches[0]['pr_numbers'], [1, 2, 3])

    def test_smart_mode_soft_disjoint_allows_shared_boilerplate_only(self):
        rows = [
            pr(10, ['src/core/A.cpp', 'CMakeLists.txt', 'docs/dev/debt/DEBT.md']),
            pr(11, ['src/core/B.cpp', 'CMakeLists.txt', 'docs/dev/debt/DEBT.md']),
        ]
        batches = generator.generate_batches(rows, [], smart_mode=True)
        self.assertEqual(len(batches), 1)
        self.assertEqual(batches[0]['batch_type'], 'soft_disjoint')
        self.assertEqual(batches[0]['pr_numbers'], [10, 11])

    def test_smart_mode_excludes_files_locked_by_active_claimed_open_prs(self):
        rows = [
            pr(1, ['locked.cpp'], github_state='open', status='in-review'),
            pr(2, ['locked.cpp']),
            pr(3, ['locked.cpp']),
            pr(4, ['free_a.cpp']),
            pr(5, ['free_b.cpp']),
        ]
        existing = [{'id': 'claimed-1', 'status': 'claimed', 'claimed_by': 'owner', 'pr_numbers': [1]}]
        batches = generator.generate_batches(rows, existing, smart_mode=True)
        self.assertEqual([b['pr_numbers'] for b in batches], [[4, 5]])


if __name__ == '__main__':
    unittest.main()

