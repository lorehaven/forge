#!/usr/bin/env python3

import json
from datetime import datetime, timezone

from vllm.model_executor.models import ModelRegistry
import vllm


OUTPUT_PATH = "vllm_architectures.json"


def main():
    architectures = sorted(ModelRegistry.get_supported_archs())

    payload = {
        "vllm_version": vllm.__version__,
        "generated_at": datetime.now(timezone.utc).isoformat(),
        "architectures": architectures,
    }

    with open(OUTPUT_PATH, "w", encoding="utf-8") as f:
        json.dump(payload, f, indent=2, sort_keys=True)

    print(f"wrote {len(architectures)} architectures to {OUTPUT_PATH}")


if __name__ == "__main__":
    main()
