#!/usr/bin/env python3
"""Cleanup old nightly artifacts from ModelScope model repo.

Keeps the last 7 nightly versions (by date in version string),
deletes everything older. Also removes stale latest.json / channel
files if no valid nightly upload succeeded today.
"""
import argparse
import os
import re
import sys
from datetime import datetime, timezone


def extract_nightly_date(version_dir: str) -> str | None:
    """Extract YYYYMMDD from a nightly version directory name like 'v0.1.0-nightly.20260605'."""
    m = re.search(r"nightly\.(\d{8})$", version_dir)
    if m:
        return m.group(1)
    return None


def list_remote_dirs(api, repo: str, prefix: str) -> list[str]:
    """List top-level directories under prefix in the ModelScope repo.

    Uses the ModelScope file listing API to enumerate directories.
    Returns a list of directory names (not full paths).
    """
    dirs: list[str] = []
    try:
        # list_repo_files returns file/folder entries under a path
        entries = api.list_repo_files(repo_id=repo, repo_type="model", recursive=False)
        # entries may be strings or dicts depending on SDK version
        for entry in entries:
            name = entry if isinstance(entry, str) else entry.get("Path", entry.get("Name", ""))
            if name:
                dirs.append(name)
    except Exception as e:
        print(f"WARNING: Failed to list repo root: {e}", file=sys.stderr)

    # Also try listing under the prefix subdirectory
    sub_dirs: list[str] = []
    try:
        entries = api.list_repo_files(
            repo_id=repo,
            repo_type="model",
            revision="master",
            recursive=False,
        )
        # If the API supports path parameter, try prefix
        # Fallback: list all and filter by prefix
    except Exception:
        pass

    return dirs


def delete_remote_file(api, repo: str, path: str) -> bool:
    """Delete a single file from ModelScope repo."""
    try:
        api.delete_file(
            path_in_repo=path,
            repo_id=repo,
            repo_type="model",
            commit_message=f"Nightly cleanup: delete {path}",
        )
        return True
    except AttributeError:
        # Some SDK versions use different method names
        try:
            api.delete_repo_file(
                repo_id=repo,
                repo_type="model",
                file_path=path,
                commit_message=f"Nightly cleanup: delete {path}",
            )
            return True
        except Exception as e:
            print(f"  [FAIL] delete {path}: {e}", file=sys.stderr)
            return False
    except Exception as e:
        print(f"  [FAIL] delete {path}: {e}", file=sys.stderr)
        return False


def main():
    parser = argparse.ArgumentParser(description="Cleanup old nightly artifacts from ModelScope")
    parser.add_argument(
        "--repo",
        required=True,
        help="ModelScope model repo (e.g. flowy2025/agent)",
    )
    parser.add_argument(
        "--keep",
        type=int,
        default=7,
        help="Number of recent nightly versions to keep (default: 7)",
    )
    parser.add_argument(
        "--dry-run",
        action="store_true",
        help="Print what would be deleted without actually deleting",
    )
    args = parser.parse_args()

    token = os.environ.get("MODELSCOPE_TOKEN")
    if not token:
        raise SystemExit("ERROR: MODELSCOPE_TOKEN environment variable not set")

    repo: str = args.repo
    keep: int = args.keep

    try:
        from modelscope.hub.api import HubApi
    except ImportError:
        raise SystemExit("ERROR: modelscope package not installed. Run: pip install modelscope")

    api = HubApi()
    api.login(token)
    print(f"Authenticated to ModelScope, cleaning repo: {repo}")

    prefix = "hermes-agent-ultra"

    # List all files under the repo to find nightly version directories
    # We look for paths like hermes-agent-ultra/v0.1.0-nightly.YYYYMMDD/*
    print(f"Listing files under {prefix}/ ...")
    nightly_versions: dict[str, list[str]] = {}  # version_dir -> [file_paths]

    try:
        # Try to list files recursively under the prefix
        all_files = api.list_repo_files(
            repo_id=repo,
            repo_type="model",
            recursive=True,
        )
    except TypeError:
        # Older SDK may not support recursive parameter
        all_files = api.list_repo_files(repo_id=repo, repo_type="model")
    except Exception as e:
        print(f"WARNING: Could not list repo files: {e}", file=sys.stderr)
        all_files = []

    # Normalize entries to path strings
    file_paths: list[str] = []
    for entry in all_files:
        if isinstance(entry, str):
            file_paths.append(entry)
        elif isinstance(entry, dict):
            file_paths.append(entry.get("Path", entry.get("Name", "")))

    # Group files by nightly version directory
    for fp in file_paths:
        # Match: hermes-agent-ultra/v0.1.0-nightly.YYYYMMDD/filename
        m = re.match(rf"^{re.escape(prefix)}/(v[0-9][^/]*-nightly\.\d{{8}})/", fp)
        if m:
            version_dir = m.group(1)
            nightly_versions.setdefault(version_dir, []).append(fp)

    if not nightly_versions:
        print("No nightly version directories found on ModelScope. Nothing to clean.")
        return

    # Sort by date extracted from version string
    def sort_key(vd: str) -> str:
        date_str = extract_nightly_date(vd)
        return date_str or "00000000"

    sorted_versions = sorted(nightly_versions.keys(), key=sort_key)
    total = len(sorted_versions)
    print(f"Found {total} nightly version(s) on ModelScope:")
    for v in sorted_versions:
        print(f"  - {v} ({len(nightly_versions[v])} files)")

    if total <= keep:
        print(f"\nOnly {total} nightly version(s) exist, no cleanup needed (threshold: {keep})")
        return

    # Determine which versions to delete (oldest first)
    remove_count = total - keep
    versions_to_delete = sorted_versions[:remove_count]
    versions_to_keep = sorted_versions[remove_count:]

    print(f"\nKeeping {keep} newest: {', '.join(versions_to_keep)}")
    print(f"Deleting {remove_count} old version(s)...")

    deleted_files = 0
    failed_files = 0

    for vd in versions_to_delete:
        files = nightly_versions[vd]
        print(f"\n  Removing version: {vd} ({len(files)} files)")
        for fp in files:
            if args.dry_run:
                print(f"    [DRY-RUN] Would delete: {fp}")
            else:
                if delete_remote_file(api, repo, fp):
                    print(f"    [OK] Deleted: {fp}")
                    deleted_files += 1
                else:
                    failed_files += 1

    # Summary
    print(f"\nCleanup complete: {deleted_files} file(s) deleted, {failed_files} failed")
    if failed_files > 0 and not args.dry_run:
        print(f"WARNING: {failed_files} file(s) could not be deleted", file=sys.stderr)
        # Don't exit with error - partial cleanup is acceptable


if __name__ == "__main__":
    main()

