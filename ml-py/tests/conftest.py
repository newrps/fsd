"""ml-py 모듈을 import 가능하게 sys.path 조정."""

from __future__ import annotations

import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent.parent))
