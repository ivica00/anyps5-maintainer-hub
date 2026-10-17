"""Compatibility entry point: refresh verified GitHub data instead of importing stale JSON.

The old import could overwrite live PR metadata and bypass transactional coordination.
Requires GitHub CLI authentication and SUPABASE_SERVICE_ROLE_KEY.
"""
from sync_service import sync

if __name__ == '__main__':
    sync()
