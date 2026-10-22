import unittest
import board_snapshot


class BoardProjection(unittest.TestCase):
    def fixture(self):
        mapping = {'RM-RUST': {'number': 1}, 'R01': {'number': 2}, 'R02': {'number': 3}}
        def issue(n, state, flow, body=''):
            return {'number': n, 'state': state, 'labels': [{'name':'flow:'+flow}], 'body':body,
                    'title':'Synthetic '+str(n), 'url':'https://example.invalid/issues/'+str(n), 'updatedAt':'2026-01-01T00:00:00Z'}
        return mapping, [issue(1,'OPEN','integrating'),issue(2,'OPEN','developing','Requires #3\n- [ ] Whole flow remains\nFlow-Blocked: waiting for real evidence'),issue(3,'CLOSED','done')]

    def test_full_scope_closed_dependencies_and_partial_evidence_stay_distinct(self):
        mapping, issues = self.fixture()
        value = board_snapshot.projection(issues,mapping,{'accepted':{'issue':2,'status':'finite flow accepted'}},'now')
        self.assertEqual((value['total'],value['remaining'],value['complete']),(2,1,1))
        self.assertEqual(value['issues'][0]['remaining'],['Whole flow remains'])
        self.assertEqual(value['issues'][0]['dependencies'],[3])
        self.assertEqual(value['issues'][0]['open_dependencies'],[])
        self.assertEqual(value['issues'][0]['evidence'][0]['status'],'finite flow accepted')
        self.assertEqual(value['issues'][0]['state'],'open')

    def test_incomplete_remote_scope_is_not_published_as_complete(self):
        mapping, issues = self.fixture()
        with self.assertRaisesRegex(ValueError,'incomplete'):
            board_snapshot.projection(issues[:-1],mapping,{},'now')

    def test_invalid_github_state_is_not_replaced_by_local_evidence(self):
        mapping, issues = self.fixture();issues[1]['state']='CLOSED'
        with self.assertRaisesRegex(ValueError,'mismatch'):
            board_snapshot.projection(issues,mapping,{'accepted':{'issue':2}},'now')


if __name__ == '__main__':
    unittest.main()
