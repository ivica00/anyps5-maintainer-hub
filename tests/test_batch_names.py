import unittest
from generate_batches_backend import generate_batches


def pr(number):
    return {'number': number, 'files': [f'{number}.cpp'], 'github_state': 'open', 'category': 'Core'}


class BatchNamesTests(unittest.TestCase):
    def test_titles_are_numbered_and_preserved_for_same_membership(self):
        initial = generate_batches([pr(1), pr(2)], [])
        self.assertEqual(initial[0]['title'], 'Batch 1')
        initial[0]['title'] = 'Batch 42'
        self.assertEqual(generate_batches([pr(2), pr(1)], initial)[0]['title'], 'Batch 42')

    def test_deleted_batch_recovers_name_from_registry(self):
        initial = generate_batches([pr(1)], [])
        names = [{'batch_id': initial[0]['id'], 'batch_number': 87}]
        self.assertEqual(generate_batches([pr(1)], [], names)[0]['title'], 'Batch 87')

    def test_new_names_follow_reserved_numbers(self):
        names = [{'batch_id': 'historical-batch', 'batch_number': 400}]
        self.assertEqual(generate_batches([pr(1)], [], names)[0]['title'], 'Batch 401')
