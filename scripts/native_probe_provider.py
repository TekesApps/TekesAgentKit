"""Intent selection for owned native-window fixtures, never application routing."""
import json
from pathlib import Path
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


def directory_probe_path(body):
    """Read the owned fixture's exact user marker without tokenizing its path.

    JSON serialization and whitespace splitting would corrupt SSD volume names,
    Unicode, quotes or backslashes. Assistant/tool quotations are not requests.
    """
    history = body.get('input', body.get('messages', []))
    if isinstance(history, str):
        candidates = [{'role': 'user', 'content': history}]
    elif isinstance(history, list):
        candidates = [m for m in reversed(history) if isinstance(m, dict) and m.get('role') == 'user']
    else:
        return None
    prefix = 'ENDPOINT_CHANGE_DIRECTORY='
    for message in candidates:
        content = message.get('content')
        texts = [content] if isinstance(content, str) else [
            part.get('text') for part in content if isinstance(part, dict)
            and part.get('type') in ('text', 'input_text')
        ] if isinstance(content, list) else []
        for text in texts:
            if isinstance(text, str) and text.startswith(prefix):
                path = text[len(prefix):]
                return path if path and '\0' not in path and Path(path).is_absolute() else None
    return None
