const fs = require('node:fs');
const {execFileSync} = require('node:child_process');
fs.mkdirSync('assets', {recursive:true});
execFileSync(process.execPath, ['node_modules/@tailwindcss/cli/dist/index.mjs',
    '-i', 'styles.css', '-o', 'assets/app.css', '--minify'], {stdio:'inherit'});
fs.copyFileSync('node_modules/@supabase/supabase-js/dist/umd/supabase.js', 'assets/supabase.js');
fs.copyFileSync('node_modules/@supabase/supabase-js/LICENSE', 'assets/LICENSE.supabase');
