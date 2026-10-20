"""Intent selection for owned native-window fixtures, never application routing."""
import json
MARKERS = ('ENDPOINT_CANCEL_NATIVE_PROOF', 'ENDPOINT_ERROR_NATIVE_PROOF', 'ENDPOINT_RECOVER_NATIVE_PROOF')

def native_probe_intent(body):
    history = body.get('input', body.get('messages', []))
    candidates = (message for message in reversed(history) if isinstance(message, dict) and message.get('role') == 'user') if isinstance(history, list) else [history]
    # Harness-injected user-role contexts can follow the actual input. Select
    # the newest explicit fixture marker, so recovery outranks historic errors.
    for message in candidates:
        serialized = json.dumps(message)
        for marker in MARKERS:
            if marker in serialized:
                return marker
    return None
