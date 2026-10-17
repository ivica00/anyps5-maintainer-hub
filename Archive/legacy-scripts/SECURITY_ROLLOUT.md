# Security rollout

1. Replace the exposed Supabase service-role credential in the Supabase dashboard. Removing it from files does not revoke it or remove Git history. Review access logs and historical deployments.
2. Provide SUPABASE_SERVICE_ROLE_KEY through the process environment for Python imports/sync. These scripts do not automatically load .env. Never put this value in browser code.
3. Back up the database, then apply supabase/migrations/20261016_secure_batches.sql followed by supabase/migrations/20261016_sync_and_regenerate.sql in Supabase SQL Editor. Apply before deploying this frontend: claims now require the claim_batch/release_batch RPCs.
4. Add each approved maintainer's Auth UUID to public.hub_maintainers using the SQL Editor. The browser's name list does not grant database access.
5. Existing claims intentionally remain locked until an administrator verifies their owners and sets batches.claimed_user_id and the matching prs.assigned_user_id. Do not infer ownership from editable user metadata.
6. Redeploy GitHub Pages so only index.html, app.js and prs_data.js are published. Remove old artifacts if still accessible. The local server also serves only those public assets.

Production rollout completed on 16 October 2026 after local and database checks. All migrations in supabase/migrations/ were applied with checksum tracking. Private pre-migration backups are stored in the ignored .local-backups/ directory. One existing approved maintainer was enrolled using their verified GitHub identity, and the existing claimed batch retained its membership and owner.

Checks passed for anonymous write denial, authenticated ownership, note preservation, successful regeneration, and concurrent claims of identical and overlapping batches in isolated disposable schemas. Rollback-only integration tests also passed against the production PostgreSQL schema.

After applying all migrations and adding maintainer UUIDs, run `python sync_service.py`, then `python generate_batches_backend.py`. GitHub CLI authentication and the service-role environment variable are required. The compatibility command `python upload_to_supabase.py` now runs the verified GitHub sync instead of importing stale local JSON.

Sync uses explicit GitHub states and complete file lists. A failed GitHub fetch writes nothing. The database applies the fetched metadata and states in one transaction without replacing notes or owners. A changed head SHA resets the checklist and returns assigned work to review. Closed/merged PRs remain stored for history but cannot be claimed.

Regeneration preserves claimed and legacy-owned batches, uses membership-derived IDs, and atomically replaces only available batches. If notes, ownership or any snapshot data changed during calculation, the operation rolls back and asks the operator to retry. Full PR metadata snapshots are sent only from the administrative script.

Checks: `python -m unittest discover -s tests -p "test_*.py"` and `node tests/frontend.test.cjs`. The optional tests/test_database.py test validates both migrations, role permissions, owner checks, rejected repeat claims, state reconciliation and note preservation inside a transaction that always rolls back. It passed against the connected PostgreSQL database. Concurrent session checks and the production deployment also passed. Public and service-role PR reads both succeeded; HTTP 401 on the anonymous OpenAPI root did not indicate an invalid API key.

Live sync reconciled 830 open, 19 merged and 7 closed stored PRs against 5,892 explicit GitHub states. It published 262 available batches and preserved the single claimed batch. All pre-existing notes were compared with the backup and preserved. Completed PRs retain history without blocking new assignments to the same files. Dashboard totals count open PRs.

Pinned local browser assets were built and published. The dependency audit reports zero known vulnerabilities. Public page, CSS and JavaScript requests return 200; server scripts, .env and the local password filename return 404.

Remaining manual action: replace the previously exposed Supabase service-role credential in the dashboard and update the administrative process environment. Removing it from source and the live website does not revoke existing copies or erase Git history. GitHub pagination is complete but a snapshot is not an immutable GitHub transaction: always recheck current CI, approvals and mergeability before merging.
