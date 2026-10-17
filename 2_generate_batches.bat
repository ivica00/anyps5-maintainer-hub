@echo off
cd /d "%~dp0"
title AnyPS5 - Sync PRs and Generate Smart Minibatches
echo ====================================================================
echo  [Step 1/2] Syncing Open PRs from GitHub to Supabase...
echo ====================================================================
echo.
python sync_service.py --incremental
if %ERRORLEVEL% NEQ 0 (
    echo.
    echo [ERROR] PR Sync failed with exit code %ERRORLEVEL%. Aborting batch generation.
    pause
    exit /b %ERRORLEVEL%
)

echo.
echo ====================================================================
echo  [Step 2/2] Generating 3-Lane Smart Batches (Clusters + Disjoint)...
echo ====================================================================
echo.
python generate_batches_backend.py
if %ERRORLEVEL% NEQ 0 (
    echo.
    echo [ERROR] Batch generation failed with exit code %ERRORLEVEL%.
) else (
    echo.
    echo [OK] Smart batches successfully generated and uploaded to Supabase!
)
echo.
pause
