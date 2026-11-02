# SPDX-License-Identifier: MIT OR Apache-2.0
"""Loopback-only, text-only SystemOne service for the pinned Clef release."""
import json
from http.server import BaseHTTPRequestHandler, HTTPServer
from pathlib import Path
import sys
import os
import time

MODEL = 'Cloudflare/clef-flash'
REVISION = 'fb4417d31c1d0f9e7e77f1141b40aaede7491e00'
MAX_BYTES = 262144
MAX_TOKENS = 16384


def validate_request(request, processor, encode_record, origin=None):
    if origin is not None:
        raise ValueError('Browser origins cannot submit model work')
    if not isinstance(request, dict) or request.get('model') != MODEL or 'state' not in request:
        raise ValueError('Expected the pinned model and a state')
    if set(request) != {'model', 'state', 'questions'}:
        raise ValueError('Only text state and questions are supported')
    questions = request['questions']
    if not isinstance(questions, dict) or not questions:
        raise ValueError('At least one question is required')
    for question in questions.values():
        if not isinstance(question, dict) or question.get('type') != 'choice':
            raise ValueError('Only choice questions are supported')
        criteria = question.get('criteria')
        if not isinstance(criteria, dict) or len(criteria) < 2 or not all(isinstance(k, str) and isinstance(v, str) and k and v for k, v in criteria.items()):
            raise ValueError('Choice criteria must contain at least two named descriptions')
    # The official encoder truncates state at max_length. Encode with a higher limit
    # first so an overlong request is refused instead of silently changing the evidence.
    encoded = encode_record(processor.tokenizer, request, max_length=MAX_BYTES * 2, processor=processor)
    if len(encoded.input_ids) > MAX_TOKENS:
        raise ValueError('Complete decision context exceeds the token limit')


def serve(model, processor, encode_record, systemone, runtime=None):
    import torch
    runtime = runtime or {}
    class Handler(BaseHTTPRequestHandler):
        def do_GET(self):
            body = json.dumps(runtime).encode('utf-8')
            self.send_response(200 if self.path == '/health' else 404)
            self.send_header('Content-Type', 'application/json')
            self.send_header('Content-Length', str(len(body)))
            self.end_headers()
            self.wfile.write(body)

        def do_POST(self):
            try:
                self.connection.settimeout(30)
                if self.path != '/v1/systemone':
                    raise ValueError('Unknown endpoint')
                length = int(self.headers.get('Content-Length', '0'))
                if length <= 0 or length > MAX_BYTES:
                    raise ValueError('Invalid request size')
                request = json.loads(self.rfile.read(length))
                validate_request(request, processor, encode_record, self.headers.get('Origin'))
                torch.cuda.synchronize()
                torch.cuda.reset_peak_memory_stats()
                started = time.perf_counter()
                result = systemone(model, processor, request, max_length=MAX_TOKENS)
                torch.cuda.synchronize()
                result['runtime'] = dict(revision=REVISION, seconds=time.perf_counter()-started,
                    peak_allocated_mib=torch.cuda.max_memory_allocated()/2**20,
                    peak_reserved_mib=torch.cuda.max_memory_reserved()/2**20)
                status = 200
            except (ValueError, TypeError, KeyError, json.JSONDecodeError) as error:
                status, result = 400, {'error': str(error)}
            except Exception:
                status, result = 500, {'error': 'Decision worker failed'}
            body = json.dumps(result, allow_nan=False).encode('utf-8')
            self.send_response(status)
            self.send_header('Content-Type', 'application/json')
            self.send_header('Content-Length', str(len(body)))
            self.end_headers()
            self.wfile.write(body)

    HTTPServer(('127.0.0.1', 8008), Handler).serve_forever()


def main():
    started = time.perf_counter()
    import torch
    from huggingface_hub import snapshot_download
    from transformers import BitsAndBytesConfig

    path = Path(snapshot_download(MODEL, revision=REVISION, local_files_only=True))
    sys.path.insert(0, str(path))
    from joint_schema_model import encode_record, load_release_model, systemone

    quantization = BitsAndBytesConfig(load_in_4bit=True, bnb_4bit_quant_type='nf4',
                                      bnb_4bit_compute_dtype=torch.bfloat16)
    model, processor = load_release_model(path, device='cuda', quantization_config=quantization)
    torch.cuda.synchronize()
    runtime = dict(model=MODEL, revision=REVISION, pid=os.getpid(), cold_start_seconds=time.perf_counter()-started,
        allocated_mib=torch.cuda.memory_allocated()/2**20, head_dtype=str(next(model.head.parameters()).dtype),
        owner=os.environ.get('SPEARFIA_CLEF_OWNER'))
    print(json.dumps(runtime), flush=True)
    serve(model, processor, encode_record, systemone, runtime)


if __name__ == '__main__':
    main()
