#!/usr/bin/env python3
"""Verify platform fixture copies do not drift from contracts/fixtures."""

from __future__ import annotations

import json
import sys
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
CONTRACTS = ROOT / "contracts" / "fixtures"
ANDROID_FIXTURES = ROOT / "android" / "app" / "src" / "test" / "resources" / "fixtures"
LINUX_FIXTURES = ROOT / "linux" / "receiver" / "tests" / "fixtures"


def canonical_json(path: Path) -> str:
    with path.open("r", encoding="utf-8") as handle:
        data = json.load(handle)
    return json.dumps(data, sort_keys=True, separators=(",", ":"))


def check_platform(platform: str, directory: Path) -> list[str]:
    errors: list[str] = []
    if not directory.exists():
        errors.append(f"{platform}: missing fixture directory {directory.relative_to(ROOT)}")
        return errors

    for contract_path in sorted(CONTRACTS.glob("*.json")):
        platform_path = directory / contract_path.name
        if not platform_path.exists():
            errors.append(f"{platform}: missing fixture copy {platform_path.relative_to(ROOT)}")
            continue

        if canonical_json(contract_path) != canonical_json(platform_path):
            errors.append(
                f"{platform}: fixture drift for {contract_path.name} "
                f"({platform_path.relative_to(ROOT)} differs from {contract_path.relative_to(ROOT)})"
            )

    return errors


def main() -> int:
    errors = []
    errors.extend(check_platform("android", ANDROID_FIXTURES))
    errors.extend(check_platform("linux", LINUX_FIXTURES))

    if errors:
        for error in errors:
            print(error, file=sys.stderr)
        return 1

    print("contract fixture copies match contracts/fixtures")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

