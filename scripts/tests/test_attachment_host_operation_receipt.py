import copy
import sys
import importlib.util
from pathlib import Path
import unittest

SCRIPT = Path(__file__).resolve().parents[1] / 'test-real-attachment-host-operation.py'
sys.path.insert(0, str(SCRIPT.parent))
spec = importlib.util.spec_from_file_location('attachment_host_runner', SCRIPT)
runner = importlib.util.module_from_spec(spec)
spec.loader.exec_module(runner)


def receipt():
    return {'offscreen': True, 'nativeInteraction': False, 'uploadProof': {
        'publicEntry': True, 'sharedHostSelectionOperation': True, 'hostCompletionApplied': True,
        'hostBoundedFileRead': True, 'receiptValidated': True, 'originalDraft': True,
        'otherDraftPreserved': True, 'oneShotUpload': True, 'componentSend': True,
        'promptSettled': True, 'attachmentOnlySend': True, 'attachmentOnlyVisible': True,
        'freshJournalAfterReconnect': True, 'reconnectNoDuplicate': True, 'durableInputCount': 1,
        'attachmentOnlyInputCount': 1, 'systemPickerExercised': False, 'nativeInteraction': False,
        'inlineImage': False, 'publicByteRead': True, 'readUnavailable': False,
        'foreignSessionReadRejected': True}}


class AttachmentHostReceiptTests(unittest.TestCase):
    def test_accepts_current_shared_host_profile_but_rejects_old_approximate_receipt(self):
        value = receipt()
        self.assertIs(runner.validate_upload_receipt(value, 'pi'), value['uploadProof'])
        old = copy.deepcopy(value)
        del old['uploadProof']['sharedHostSelectionOperation']
        with self.assertRaises(ValueError):
            runner.validate_upload_receipt(old, 'pi')

    def test_dsh_absence_of_byte_read_is_not_a_successful_foreign_rejection(self):
        value = receipt()
        with self.assertRaises(ValueError):
            runner.validate_upload_receipt(value, 'dsh')
        value['uploadProof'].update(publicByteRead=False, readUnavailable=True,
                                    foreignSessionReadRejected=None)
        runner.validate_upload_receipt(value, 'dsh')
        value['uploadProof']['foreignSessionReadRejected'] = True
        with self.assertRaises(ValueError):
            runner.validate_upload_receipt(value, 'dsh')

    def test_rejects_picker_claim_boolean_counts_and_unknown_profile(self):
        for key, item in [('systemPickerExercised', True), ('durableInputCount', True),
                          ('attachmentOnlyInputCount', 0), ('nativeInteraction', True)]:
            value = receipt()
            value['uploadProof'][key] = item
            with self.subTest(key=key), self.assertRaises(ValueError):
                runner.validate_upload_receipt(value, 'native')
        with self.assertRaises(ValueError):
            runner.validate_upload_receipt(receipt(), 'unknown')


if __name__ == '__main__':
    unittest.main()
