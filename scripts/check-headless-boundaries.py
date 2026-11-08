#!/usr/bin/env python3
"""Keep headless production dependencies independent of UI, server internals and SQLite."""
import argparse
import json
from pathlib import Path
import subprocess
from acceptance_tools import headless_dependencies, require, source_files


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--root', type=Path, default=Path(__file__).resolve().parent.parent)
    parser.add_argument('--metadata', type=Path, help='Resolved cargo metadata for this exact root')
    args = parser.parse_args()
    root = args.root.resolve(strict=True)
    tracked, inventory = source_files(root)
    swift = [name for name in tracked if name.endswith('.swift') and (root / name).is_file()]
    require(not swift, f'Swift sources are not allowed in AgentKit: {swift}')
    metadata = json.loads(args.metadata.read_text() if args.metadata else subprocess.check_output(
        ['cargo', 'metadata', '--locked', '--no-deps', '--format-version', '1'], cwd=root))
    require(Path(metadata['workspace_root']).resolve() == root, 'metadata workspace root mismatch')
    headless_dependencies(metadata, root)
    print(f'HEADLESS_DEPENDENCY_BOUNDARIES_OK inventory={inventory}')


if __name__ == '__main__':
    main()
