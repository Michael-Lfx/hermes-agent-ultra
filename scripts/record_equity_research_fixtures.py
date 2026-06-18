#!/usr/bin/env python3
"""Record equity research golden fixtures from UZI fin_models.py."""

from __future__ import annotations

import json
import sys
from pathlib import Path

UZI_SCRIPTS = Path(r"c:\code\github\UZI-Skill\skills\deep-analysis\scripts")
OUT = Path(__file__).resolve().parents[1] / "crates/hermes-parity-tests/fixtures/trading_research/models_golden.json"
FETCHER_OUT = (
    Path(__file__).resolve().parents[1]
    / "crates/hermes-parity-tests/fixtures/trading_research_fetch/fetcher_golden.json"
)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
