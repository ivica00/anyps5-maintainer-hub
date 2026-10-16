"""Small, dependency-free Supabase client for administrative scripts."""
import json
import os
import urllib.request


def request(path, payload=None):
    key = os.environ['SUPABASE_SERVICE_ROLE_KEY']
    base = os.environ.get('SUPABASE_URL', 'https://nzaxslgwsygpgwqcllpi.supabase.co')
    req = urllib.request.Request(
        f'{base}/rest/v1/{path}',
        data=None if payload is None else json.dumps(payload).encode('utf-8'),
        headers={'apikey': key, 'Authorization': f'Bearer {key}',
                 'Content-Type': 'application/json'})
    with urllib.request.urlopen(req, timeout=120) as response:
        body = response.read()
        return json.loads(body) if body else None


def read_all(query):
    """Continue after short pages too: the server may cap pages below our limit."""
    rows = []
    while True:
        page = request(f'{query}&limit=500&offset={len(rows)}')
        if not isinstance(page, list):
            raise ValueError('Expected a database row array')
        if not page:
            return rows
        rows.extend(page)
