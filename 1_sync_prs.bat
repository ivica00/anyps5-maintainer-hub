@echo off
cd /d "%~dp0"
title AnyPS5 - Sync Open PRs with GitHub & Supabase
echo ====================================================================
echo  [1/1] Syncing Open PRs from GitHub to Supabase (Incremental SHA)...
echo        Merged and closed PRs will be automatically cleaned up.
echo ====================================================================
echo.
python sync_service.py --incremental
if %ERRORLEVEL% NEQ 0 (
    echo.
    echo [ERROR] Sync failed with exit code %ERRORLEVEL%.
) else (
    echo.
    echo [OK] Supabase PR table is now 100%% synced with GitHub!
)
echo.
pause
