import sys
from pathlib import Path
import unittest

sys.path.insert(0,str(Path(__file__).resolve().parents[1]))
from performance_process_sampler import classify


def row(pid, ppid, command, start='Mon Oct 26 01:00:00 2026'):
    return {'pid':pid,'ppid':ppid,'command':command,'startIdentity':start,'rssKiB':10}


class OwnedRssTests(unittest.TestCase):
    def test_mutable_pi_title_keeps_actual_node_executable_in_harness_tree(self):
        rows={2:row(2,1,'consumer'),3:row(3,2,'pi'),4:row(4,3,'read-tool')}
        rows[3]['executablePath']='/fixed/pi-runtime/bin/node'
        roots={2:{'role':'agentkit','startIdentity':rows[2]['startIdentity']}}
        owned=classify(rows,roots,{},pi_embedded=True)
        self.assertEqual({pid:r['role'] for pid,r in owned.items()},
                         {2:'agentkit',3:'harness',4:'harness'})

    def test_embedded_pi_tree_is_harness_while_consumer_and_foreign_are_separate(self):
        rows={1:row(1,0,'wrapper'),2:row(2,1,'consumer'),3:row(3,2,'node'),
              4:row(4,3,'read-tool'),5:row(5,999,'unrelated')}
        roots={1:{'role':'provider','startIdentity':rows[1]['startIdentity']},
               2:{'role':'agentkit','startIdentity':rows[2]['startIdentity']}}
        owned=classify(rows,roots,{},pi_embedded=True)
        self.assertEqual({pid:r['role'] for pid,r in owned.items()},
                         {1:'provider',2:'agentkit',3:'harness',4:'harness'})

    def test_orphan_identity_is_retained_but_reused_pid_never_becomes_owned(self):
        old='Mon Oct 26 01:00:00 2026'
        rows={3:row(3,1,'node',old),4:row(4,1,'foreign','Mon Oct 26 01:02:00 2026')}
        known={(3,old):'harness',(4,old):'harness'}
        self.assertEqual(set(classify(rows,{},known)),{3})


if __name__=='__main__':
    unittest.main()
