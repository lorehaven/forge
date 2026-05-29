FROM python:3.11-slim

RUN pip install --no-cache-dir requests

RUN echo 'import requests\n\
import argparse\n\
import sys\n\
\n\
def main():\n\
    parser = argparse.ArgumentParser()\n\
    parser.add_argument("--host", required=True)\n\
    parser.add_argument("--port", type=int, default=8000)\n\
    parser.add_argument("--model", help="Model name to test chat completion")\n\
    args = parser.parse_args()\n\
\n\
    base_url = f"http://{args.host}:{args.port}/v1"\n\
    \n\
    print(f"Checking vLLM models at {base_url}/models...")\n\
    try:\n\
        resp = requests.get(f"{base_url}/models", timeout=10)\n\
        resp.raise_for_status()\n\
        models = resp.json()\n\
        print(f"Available models: {models}")\n\
    except Exception as e:\n\
        print(f"Failed to connect: {e}")\n\
        sys.exit(1)\n\
\n\
    if args.model:\n\
        print(f"\\nSending test chat completion for {args.model}...")\n\
        payload = {\n\
            "model": args.model,\n\
            "messages": [{"role": "user", "content": "Say '\''Connection Verified'\''"}],\n\
            "max_tokens": 10\n\
        }\n\
        try:\n\
            resp = requests.post(f"{base_url}/chat/completions", json=payload, timeout=30)\n\
            resp.raise_for_status()\n\
            result = resp.json()\n\
            print(f"Response: {result['\''choices'\''][0]['\''message'\'']['\''content'\'']}")\n\
        except Exception as e:\n\
            print(f"Chat completion failed: {e}")\n\
            sys.exit(1)\n\
\n\
if __name__ == "__main__":\n\
    main()' > /usr/local/bin/vllm-test.py && chmod +x /usr/local/bin/vllm-test.py

ENTRYPOINT ["python", "/usr/local/bin/vllm-test.py"]
