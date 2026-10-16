/**
 * AnyPS5 Maintainer Workspace - Supabase Realtime Backend Integration
 */

const SUPABASE_URL = 'https://nzaxslgwsygpgwqcllpi.supabase.co';
const SUPABASE_ANON_KEY = 'eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.eyJpc3MiOiJzdXBhYmFzZSIsInJlZiI6Im56YXhzbGd3c3lncGd3cWNsbHBpIiwicm9sZSI6ImFub24iLCJpYXQiOjE3OTIxNTU2NjksImV4cCI6MjEwNzczMTY2OX0.G_sibL-5v-wxWagMeY1eAXZXm2b5Bsh0nziN94a4moY';

// Initialize Supabase Client
let supabase = null;
if (window.supabase) {
    supabase = window.supabase.createClient(SUPABASE_URL, SUPABASE_ANON_KEY);
}

const state = {
    allPRs: [],
    currentMaintainer: 'boykopovar',
    activeView: 'mine', // 'mine' or 'repo'
    myStatusFilter: 'all', // 'all', 'pending', 'approved', 'changes', 'ready'
    selectedClaimPRs: new Set(),
    activeInspectPR: null,
    supabaseConnected: false
};

// Initialization
document.addEventListener('DOMContentLoaded', () => {
    loadPersistentUser();
    loadLocalFallbackData();
    setupMaintainerSelector();
    setupRepoFilters();
    renderMyPRs();
    renderRepoTable();

    // Connect to Supabase and subscribe to Realtime
    initSupabase();
});

// Load persistent maintainer identity
function loadPersistentUser() {
    try {
        const savedUser = localStorage.getItem('anyps5_current_maintainer');
        if (savedUser) state.currentMaintainer = savedUser;
    } catch (e) {
        console.warn('LocalStorage error', e);
    }
}

function savePersistentUser() {
    try {
        localStorage.setItem('anyps5_current_maintainer', state.currentMaintainer);
    } catch (e) {
        console.warn('LocalStorage error', e);
    }
}

// 1. Initial fast render from local bundle (0ms delay)
function loadLocalFallbackData() {
    if (window.ANYPS5_DATA && window.ANYPS5_DATA.prs) {
        state.allPRs = window.ANYPS5_DATA.prs.map(pr => ({
            ...pr,
            status: pr.status || 'unassigned',
            assignedReviewer: pr.assignedReviewer || 'none',
            notes: pr.notes || '',
            checklist: pr.checklist || {}
        }));
    }
}

// 2. Connect to Supabase REST & Realtime WebSocket
async function initSupabase() {
    const pill = document.getElementById('supabase-status-pill');
    if (!supabase) {
        if (pill) {
            pill.innerHTML = `<span class="w-1.5 h-1.5 rounded-full bg-amber-400"></span> Offline Cache`;
            pill.className = "text-[10px] px-2 py-0.5 rounded-full bg-amber-950 text-amber-400 border border-amber-800/80 font-mono flex items-center gap-1";
        }
        return;
    }

    try {
        if (pill) {
            pill.innerHTML = `<span class="w-1.5 h-1.5 rounded-full bg-cyan-400 animate-spin"></span> Spajanje...`;
        }

        // Fetch all PRs from Supabase
        const { data, error } = await supabase
            .from('prs')
            .select('*')
            .order('number', { ascending: false });

        if (error) throw error;

        if (data && data.length > 0) {
            state.allPRs = data.map(row => ({
                number: row.number,
                title: row.title,
                author: row.author,
                authorAvatar: row.author_avatar || `https://github.com/${row.author}.png`,
                createdAt: row.created_at,
                url: row.url || `https://github.com/boykopovar/AnyPS5/pull/${row.number}`,
                additions: row.additions || 0,
                deletions: row.deletions || 0,
                totalLines: row.total_lines || 0,
                fileCount: row.file_count || 1,
                files: row.files || [],
                category: row.category || 'Other Core Subsystems',
                hotspots: row.hotspots || [],
                riskLevel: row.risk_level || 'low',
                size: row.size || 'M',
                isDraft: row.is_draft || false,
                assignedReviewer: row.assigned_reviewer || 'none',
                status: row.status || 'unassigned',
                notes: row.notes || '',
                checklist: row.checklist || {}
            }));

            state.supabaseConnected = true;
            if (pill) {
                pill.innerHTML = `<span class="w-1.5 h-1.5 rounded-full bg-emerald-400 animate-pulse"></span> Supabase Povezan (${data.length} PR)`;
                pill.className = "text-[10px] px-2 py-0.5 rounded-full bg-emerald-950 text-emerald-400 border border-emerald-800/80 font-mono flex items-center gap-1";
            }

            renderMyPRs();
            renderRepoTable();
        }

        // 3. Odmah sinkroniziraj najnovije s GitHuba
        autoSyncFromGitHub();

        // Subscribe to Supabase Realtime changes
        subscribeToRealtime();

        // 4. Polling fallback: automatsko osvježavanje svaku minutu (60s)
        setInterval(() => {
            refreshDataFromSupabase();
        }, 60000);

    } catch (err) {
        console.error('Supabase connection error:', err);
        if (pill) {
            pill.innerHTML = `<span class="w-1.5 h-1.5 rounded-full bg-amber-400"></span> Lokalni način`;
            pill.className = "text-[10px] px-2 py-0.5 rounded-full bg-amber-950 text-amber-400 border border-amber-800/80 font-mono flex items-center gap-1";
        }
    }
}

// Osvježavanje podataka iz Supabasea (poziva se svaku minutu)
async function refreshDataFromSupabase() {
    if (!supabase) return;
    try {
        const { data, error } = await supabase
            .from('prs')
            .select('*')
            .order('number', { ascending: false });

        if (error || !data) return;

        // Ažuriraj stanje novim ili izmijenjenim zapisima
        data.forEach(row => {
            const existing = state.allPRs.find(p => p.number === row.number);
            if (existing) {
                existing.assignedReviewer = row.assigned_reviewer || 'none';
                existing.status = row.status || 'unassigned';
                existing.notes = row.notes || '';
                existing.checklist = row.checklist || {};
            } else {
                state.allPRs.unshift({
                    number: row.number,
                    title: row.title,
                    author: row.author,
                    authorAvatar: row.author_avatar || `https://github.com/${row.author}.png`,
                    createdAt: row.created_at,
                    url: row.url || `https://github.com/boykopovar/AnyPS5/pull/${row.number}`,
                    additions: row.additions || 0,
                    deletions: row.deletions || 0,
                    totalLines: row.total_lines || 0,
                    fileCount: row.file_count || 1,
                    files: row.files || [],
                    category: row.category || 'Other Core Subsystems',
                    hotspots: row.hotspots || [],
                    riskLevel: row.risk_level || 'low',
                    size: row.size || 'M',
                    isDraft: row.is_draft || false,
                    assignedReviewer: row.assigned_reviewer || 'none',
                    status: row.status || 'unassigned',
                    notes: row.notes || '',
                    checklist: row.checklist || {}
                });
            }
        });

        renderMyPRs();
        renderRepoTable();

        // 5. Automatska sinkronizacija s GitHuba: Provjeri ima li novih PR-ova na GitHubu
        autoSyncFromGitHub();
    } catch (e) {
        console.warn('1-minute refresh error:', e);
    }
}

// Provjera novih PR-ova direktno s GitHub API-ja i unos u Supabase
async function autoSyncFromGitHub() {
    if (!supabase) return;
    try {
        const res = await fetch('https://api.github.com/repos/boykopovar/AnyPS5/pulls?state=open&per_page=40');
        if (!res.ok) return;
        const ghPRs = await res.json();
        if (!Array.isArray(ghPRs)) return;

        const newPRsToInsert = [];
        for (const gp of ghPRs) {
            const exists = state.allPRs.some(p => p.number === gp.number);
            if (!exists) {
                let cat = 'PRX High-Level Modules';
                const tl = gp.title.toLowerCase();
                if (tl.includes('agc')) cat = 'AGC & Graphics';
                else if (tl.includes('shader') || tl.includes('relinker')) cat = 'Shader & Relinker';
                else if (tl.includes('kernel') || tl.includes('memory')) cat = 'Kernel & Syscalls';
                else if (tl.includes('libc')) cat = 'LibC & Base Runtime';

                newPRsToInsert.push({
                    number: gp.number,
                    title: gp.title,
                    author: gp.user ? gp.user.login : 'ghost',
                    author_avatar: gp.user ? gp.user.avatar_url : 'https://github.com/github.png',
                    created_at: gp.created_at,
                    url: gp.html_url,
                    category: cat,
                    risk_level: 'low',
                    size: 'S',
                    is_draft: gp.draft || false,
                    assigned_reviewer: 'none',
                    status: 'unassigned',
                    notes: '',
                    checklist: {}
                });
            }
        }

        if (newPRsToInsert.length > 0) {
            console.log(`Pronađeno ${newPRsToInsert.length} novih PR-ova na GitHubu. Unosim u Supabase...`);
            await supabase.from('prs').upsert(newPRsToInsert, { onConflict: 'number' });
            showToast(`Dohvaćeno ${newPRsToInsert.length} novih PR-ova s GitHuba!`);
        }
    } catch (err) {
        console.warn('GitHub auto-sync error:', err);
    }
}

// 3. Supabase Realtime WebSocket listener
function subscribeToRealtime() {
    if (!supabase) return;

    supabase
        .channel('public:prs')
        .on('postgres_changes', { event: '*', schema: 'public', table: 'prs' }, (payload) => {
            const updatedRow = payload.new;
            if (!updatedRow || !updatedRow.number) return;

            const existingPR = state.allPRs.find(p => p.number === updatedRow.number);
            if (existingPR) {
                const wasMine = existingPR.assignedReviewer === state.currentMaintainer;
                const isNowMine = updatedRow.assigned_reviewer === state.currentMaintainer;

                existingPR.assignedReviewer = updatedRow.assigned_reviewer || 'none';
                existingPR.status = updatedRow.status || 'unassigned';
                existingPR.notes = updatedRow.notes || '';
                existingPR.checklist = updatedRow.checklist || {};

                renderMyPRs();
                renderRepoTable();

                if (!wasMine && isNowMine) {
                    showToast(`Novi PR #${updatedRow.number} dodijeljen tebi!`);
                } else if (wasMine && !isNowMine) {
                    showToast(`PR #${updatedRow.number} preuzeo @${updatedRow.assigned_reviewer}`);
                }
            }
        })
        .subscribe();
}

// Setup maintainer selection
function setupMaintainerSelector() {
    const select = document.getElementById('current-maintainer-select');
    if (!select) return;

    let exists = false;
    for (let opt of select.options) {
        if (opt.value === state.currentMaintainer) {
            exists = true;
            break;
        }
    }
    if (!exists && state.currentMaintainer !== 'custom') {
        const opt = document.createElement('option');
        opt.value = state.currentMaintainer;
        opt.innerText = `@${state.currentMaintainer}`;
        select.insertBefore(opt, select.lastElementChild);
    }

    select.value = state.currentMaintainer;
    updateMaintainerLabels();

    select.addEventListener('change', (e) => {
        if (e.target.value === 'custom') {
            const customName = prompt('Unesi svoje GitHub korisničko ime:');
            if (customName && customName.trim()) {
                const clean = customName.trim().replace(/^@/, '');
                state.currentMaintainer = clean;
                const newOpt = document.createElement('option');
                newOpt.value = clean;
                newOpt.innerText = `@${clean}`;
                select.insertBefore(newOpt, select.lastElementChild);
                select.value = clean;
            } else {
                select.value = state.currentMaintainer;
                return;
            }
        } else {
            state.currentMaintainer = e.target.value;
        }
        savePersistentUser();
        updateMaintainerLabels();
        renderMyPRs();
        renderRepoTable();
        showToast(`Prijavljen kao @${state.currentMaintainer}`);
    });
}

function updateMaintainerLabels() {
    const heroName = document.getElementById('hero-maintainer-name');
    if (heroName) heroName.innerText = `@${state.currentMaintainer}`;

    const claimTag = document.getElementById('claim-user-tag');
    if (claimTag) claimTag.innerText = `@${state.currentMaintainer}`;
}

// Switch between "Moji PR-ovi" and "Cijeli repozitorij"
function switchMainView(view) {
    state.activeView = view;
    const btnMine = document.getElementById('view-tab-mine');
    const btnRepo = document.getElementById('view-tab-repo');
    const secMine = document.getElementById('view-mine-section');
    const secRepo = document.getElementById('view-repo-section');

    if (view === 'mine') {
        btnMine.classList.add('active', 'border-cyan-400', 'text-cyan-300');
        btnMine.classList.remove('text-slate-400', 'border-transparent');
        btnRepo.classList.remove('active', 'border-cyan-400', 'text-cyan-300');
        btnRepo.classList.add('text-slate-400', 'border-transparent');
        secMine.classList.remove('hidden');
        secRepo.classList.add('hidden');
        renderMyPRs();
    } else {
        btnRepo.classList.add('active', 'border-cyan-400', 'text-cyan-300');
        btnRepo.classList.remove('text-slate-400', 'border-transparent');
        btnMine.classList.remove('active', 'border-cyan-400', 'text-cyan-300');
        btnMine.classList.add('text-slate-400', 'border-transparent');
        secMine.classList.add('hidden');
        secRepo.classList.remove('hidden');
        renderRepoTable();
    }
}

// ==========================================
// VIEW 1: MOJI PR-OVI (MY ASSIGNED PRs)
// ==========================================

function renderMyPRs() {
    const container = document.getElementById('my-prs-container');
    const emptyState = document.getElementById('my-prs-empty');
    if (!container) return;

    const myPRs = state.allPRs.filter(p => p.assignedReviewer === state.currentMaintainer);
    
    // Update badge in top bar
    const badge = document.getElementById('my-pr-count-badge');
    if (badge) badge.innerText = myPRs.length;

    // Update status pills counts
    updateMyStatusCounts(myPRs);

    // Apply active filter
    let visiblePRs = myPRs;
    if (state.myStatusFilter === 'pending') {
        visiblePRs = myPRs.filter(p => p.status === 'unassigned' || p.status === 'in-review');
    } else if (state.myStatusFilter !== 'all') {
        visiblePRs = myPRs.filter(p => p.status === state.myStatusFilter);
    }

    if (myPRs.length === 0) {
        container.innerHTML = '';
        emptyState.classList.remove('hidden');
        return;
    } else {
        emptyState.classList.add('hidden');
    }

    if (visiblePRs.length === 0) {
        container.innerHTML = `
            <div class="p-8 text-center bg-slate-900/60 rounded-xl border border-slate-800 text-slate-400 text-sm">
                Nema PR-ova s odabranim statusom. <button onclick="filterMyPRs('all')" class="text-cyan-400 underline ml-1">Prikaži sve</button>
            </div>
        `;
        return;
    }

    container.innerHTML = visiblePRs.map(pr => {
        const riskBadge = getRiskBadge(pr.riskLevel);
        const catBadge = getCategoryBadge(pr.category);
        const statusBadge = getStatusBadge(pr.status);
        const hasHotspots = pr.hotspots.length > 0;

        return `
            <div class="p-4 rounded-xl bg-cardbg border border-slate-800/90 hover:border-slate-700 transition shadow-sm space-y-3">
                <div class="flex flex-col sm:flex-row sm:items-center justify-between gap-2">
                    <div class="flex items-center gap-2.5 flex-wrap">
                        <span class="font-mono font-bold text-cyan-400 text-sm">#${pr.number}</span>
                        ${catBadge}
                        ${riskBadge}
                        ${statusBadge}
                    </div>
                    <div class="text-xs text-slate-400 font-mono">
                        <span class="text-slate-300 font-semibold">${pr.fileCount} ${pr.fileCount === 1 ? 'datoteka' : 'datoteka'}</span> 
                        (<span class="text-emerald-400">+${pr.additions}</span> / <span class="text-rose-400">-${pr.deletions}</span>)
                    </div>
                </div>

                <div>
                    <h3 class="text-base font-semibold text-slate-100 hover:text-cyan-300 transition cursor-pointer" onclick="openInspectModal(${pr.number})">
                        ${escapeHtml(pr.title)}
                    </h3>
                    <div class="flex items-center gap-2 mt-1 text-xs text-slate-400">
                        <span>Autor: <strong class="text-slate-300 font-mono">@${escapeHtml(pr.author)}</strong></span>
                        <span>•</span>
                        <span>${formatRelativeTime(pr.createdAt)}</span>
                        ${hasHotspots ? `
                            <span class="px-1.5 py-0.5 rounded bg-rose-950 text-rose-300 border border-rose-800 text-[10px] font-bold">
                                🔥 Hotspot: ${escapeHtml(pr.hotspots[0].split('/').pop())}
                            </span>
                        ` : ''}
                    </div>
                </div>

                ${pr.notes ? `
                    <div class="text-xs p-2 rounded bg-slate-950 border border-slate-800 text-slate-300 flex items-start gap-2">
                        <span class="text-slate-500">📝 Bilješka:</span>
                        <span class="italic text-slate-300">${escapeHtml(pr.notes)}</span>
                    </div>
                ` : ''}

                <!-- Action Tools Row -->
                <div class="pt-2 border-t border-slate-800 flex items-center justify-between flex-wrap gap-2">
                    <div class="flex items-center gap-1.5">
                        <button onclick="openInspectModal(${pr.number})" 
                                class="px-3.5 py-1.5 rounded-lg bg-cyan-600 hover:bg-cyan-500 text-white font-bold text-xs shadow-md shadow-cyan-600/20 transition flex items-center gap-1.5">
                            <span>🧪</span> Ispitaj ovaj PR i alate
                        </button>
                        <button onclick="copyCliCheckout(${pr.number})" 
                                class="px-3 py-1.5 rounded-lg bg-slate-800 hover:bg-slate-700 text-slate-300 text-xs font-mono transition border border-slate-700 flex items-center gap-1">
                            <span>💻</span> gh checkout
                        </button>
                    </div>

                    <div class="flex items-center gap-1.5">
                        <select onchange="quickUpdateMyStatus(${pr.number}, this.value)" 
                                class="bg-slate-900 border border-slate-700 text-slate-200 text-xs rounded-lg px-2.5 py-1.5 focus:outline-none focus:border-cyan-500">
                            <option value="in-review" ${pr.status === 'in-review' || pr.status === 'unassigned' ? 'selected' : ''}>🔍 Čeka pregled</option>
                            <option value="approved" ${pr.status === 'approved' ? 'selected' : ''}>✅ Odobreno</option>
                            <option value="changes" ${pr.status === 'changes' ? 'selected' : ''}>⚠️ Zatraži izmjene</option>
                            <option value="conflict" ${pr.status === 'conflict' ? 'selected' : ''}>🚫 Konflikt</option>
                            <option value="ready" ${pr.status === 'ready' ? 'selected' : ''}>🚀 Spreman za merge</option>
                        </select>
                        <button onclick="unassignPR(${pr.number})" title="Ukloni iz mog reda" 
                                class="p-1.5 rounded-lg text-slate-500 hover:text-rose-400 hover:bg-slate-800 transition text-xs">
                            ✕
                        </button>
                    </div>
                </div>
            </div>
        `;
    }).join('');
}

function updateMyStatusCounts(myPRs) {
    const elAll = document.getElementById('my-stat-all');
    const elPending = document.getElementById('my-stat-pending');
    const elApproved = document.getElementById('my-stat-approved');
    const elChanges = document.getElementById('my-stat-changes');
    const elReady = document.getElementById('my-stat-ready');

    if (elAll) elAll.innerText = myPRs.length;
    if (elPending) elPending.innerText = myPRs.filter(p => p.status === 'unassigned' || p.status === 'in-review').length;
    if (elApproved) elApproved.innerText = myPRs.filter(p => p.status === 'approved').length;
    if (elChanges) elChanges.innerText = myPRs.filter(p => p.status === 'changes').length;
    if (elReady) elReady.innerText = myPRs.filter(p => p.status === 'ready').length;
}

function filterMyPRs(status) {
    state.myStatusFilter = status;
    document.querySelectorAll('.my-pill').forEach(btn => {
        btn.classList.remove('bg-slate-800', 'border-cyan-500/50', 'text-cyan-300');
        btn.classList.add('bg-slate-900', 'border-slate-800', 'text-slate-400');
    });
    event.currentTarget.classList.add('bg-slate-800', 'border-cyan-500/50', 'text-cyan-300');
    event.currentTarget.classList.remove('bg-slate-900', 'border-slate-800', 'text-slate-400');
    renderMyPRs();
}

async function quickUpdateMyStatus(prNumber, newStatus) {
    const target = state.allPRs.find(p => p.number === prNumber);
    if (target) target.status = newStatus;
    renderMyPRs();

    if (supabase) {
        await supabase
            .from('prs')
            .update({ status: newStatus, updated_at: new Date().toISOString() })
            .eq('number', prNumber);
    }
    showToast(`Status ažuriran: ${newStatus}`);
}

async function assignPR(prNumber, maintainerId, renderNow = true) {
    const target = state.allPRs.find(p => p.number === prNumber);
    const newStatus = target && target.status === 'unassigned' ? 'in-review' : (target?.status || 'in-review');
    
    if (target) {
        target.assignedReviewer = maintainerId;
        target.status = newStatus;
    }

    if (renderNow) {
        renderMyPRs();
        renderRepoTable();
    }

    if (supabase) {
        await supabase
            .from('prs')
            .update({ 
                assigned_reviewer: maintainerId, 
                status: newStatus,
                updated_at: new Date().toISOString() 
            })
            .eq('number', prNumber);
    }
}

async function unassignPR(prNumber) {
    const target = state.allPRs.find(p => p.number === prNumber);
    if (target) {
        target.assignedReviewer = 'none';
        target.status = 'unassigned';
    }

    renderMyPRs();
    renderRepoTable();

    if (supabase) {
        await supabase
            .from('prs')
            .update({ 
                assigned_reviewer: 'none', 
                status: 'unassigned',
                updated_at: new Date().toISOString() 
            })
            .eq('number', prNumber);
    }
    showToast(`PR #${prNumber} vraćen u repozitorij`);
}


// ==========================================
// MODAL: PREUZMI NOVE PR-OVE (CLAIM MODAL)
// ==========================================

function openClaimModal() {
    state.selectedClaimPRs.clear();
    const modal = document.getElementById('claim-modal');
    if (!modal) return;

    renderClaimList();
    modal.classList.remove('hidden');
    modal.classList.add('flex');
}

function closeClaimModal() {
    const modal = document.getElementById('claim-modal');
    if (modal) {
        modal.classList.add('hidden');
        modal.classList.remove('flex');
    }
}

function renderClaimList() {
    const container = document.getElementById('claim-pr-list');
    if (!container) return;

    // Show unassigned PRs or PRs assigned to others
    const available = state.allPRs.filter(p => p.assignedReviewer !== state.currentMaintainer).slice(0, 50);

    container.innerHTML = available.map(pr => {
        const isChecked = state.selectedClaimPRs.has(pr.number);
        return `
            <div class="flex items-center justify-between p-2.5 rounded-lg bg-slate-900 border border-slate-800 hover:border-slate-700 transition">
                <div class="flex items-center gap-2.5 flex-1 min-w-0 pr-2">
                    <input type="checkbox" ${isChecked ? 'checked' : ''} 
                           onchange="toggleClaimSelect(${pr.number}, this.checked)"
                           class="rounded bg-slate-800 border-slate-700 text-cyan-500 focus:ring-cyan-500 cursor-pointer">
                    <span class="font-mono font-bold text-cyan-400 text-xs">#${pr.number}</span>
                    <span class="text-xs text-slate-200 truncate" title="${escapeHtml(pr.title)}">${escapeHtml(pr.title)}</span>
                </div>
                <div class="flex items-center gap-2">
                    ${getRiskBadge(pr.riskLevel)}
                    <button onclick="quickClaimSingle(${pr.number})" 
                            class="px-2.5 py-1 bg-cyan-950 hover:bg-cyan-800 text-cyan-300 border border-cyan-800 rounded text-[11px] font-semibold transition">
                        + Preuzmi
                    </button>
                </div>
            </div>
        `;
    }).join('');

    updateClaimSelectionMsg();
}

function toggleClaimSelect(prNumber, checked) {
    if (checked) state.selectedClaimPRs.add(prNumber);
    else state.selectedClaimPRs.delete(prNumber);
    updateClaimSelectionMsg();
}

function updateClaimSelectionMsg() {
    const msg = document.getElementById('claim-selected-msg');
    if (msg) msg.innerText = `${state.selectedClaimPRs.size} PR-ova odabrano`;
}

function quickClaimSingle(prNumber) {
    assignPR(prNumber, state.currentMaintainer, true);
    renderClaimList();
    showToast(`Preuzet PR #${prNumber}`);
}

async function confirmSelectedClaims() {
    if (state.selectedClaimPRs.size === 0) {
        showToast('Odaberi barem jedan PR kvačicom');
        return;
    }
    const count = state.selectedClaimPRs.size;
    const selected = Array.from(state.selectedClaimPRs);
    
    selected.forEach(num => assignPR(num, state.currentMaintainer, false));
    closeClaimModal();
    renderMyPRs();
    renderRepoTable();

    if (supabase) {
        await supabase
            .from('prs')
            .update({ 
                assigned_reviewer: state.currentMaintainer, 
                status: 'in-review',
                updated_at: new Date().toISOString() 
            })
            .in('number', selected);
    }
    showToast(`Uspješno dodano ${count} PR-ova u tvoj radni red!`);
}

async function batchClaimPreset(type, count) {
    let pool = [];
    if (type === 'safe') {
        pool = state.allPRs.filter(p => p.assignedReviewer !== state.currentMaintainer && p.riskLevel === 'low');
    } else {
        pool = state.allPRs.filter(p => p.assignedReviewer !== state.currentMaintainer && p.category === type);
    }

    const toClaim = pool.slice(0, count);
    if (toClaim.length === 0) {
        showToast('Nema slobodnih PR-ova u ovoj kategoriji');
        return;
    }

    const numbers = toClaim.map(p => p.number);
    numbers.forEach(num => assignPR(num, state.currentMaintainer, false));
    closeClaimModal();
    renderMyPRs();
    renderRepoTable();

    if (supabase) {
        await supabase
            .from('prs')
            .update({ 
                assigned_reviewer: state.currentMaintainer, 
                status: 'in-review',
                updated_at: new Date().toISOString() 
            })
            .in('number', numbers);
    }
    showToast(`Preuzeto ${toClaim.length} PR-ova za @${state.currentMaintainer}!`);
}

function quickClaimSafeBatch(count) {
    batchClaimPreset('safe', count);
}


// ==========================================
// MODAL: ALATI ZA ISPITIVANJE PR-A (INSPECT MODAL)
// ==========================================

function openInspectModal(prNumber) {
    const pr = state.allPRs.find(p => p.number === prNumber);
    if (!pr) return;

    state.activeInspectPR = pr;
    const modal = document.getElementById('inspect-modal');
    if (!modal) return;

    // Header info
    document.getElementById('inspect-pr-num').innerText = `#${pr.number}`;
    document.getElementById('inspect-pr-title').innerText = pr.title;
    document.getElementById('inspect-pr-author').innerText = `@${pr.author}`;
    document.getElementById('inspect-pr-date').innerText = `${formatDate(pr.createdAt)} (${formatRelativeTime(pr.createdAt)})`;
    document.getElementById('inspect-pr-cat').innerHTML = getCategoryBadge(pr.category);
    document.getElementById('inspect-pr-risk').innerHTML = getRiskBadge(pr.riskLevel);
    document.getElementById('inspect-github-link').href = pr.url;

    // ALAT 1: Hotspots & Conflicts
    const conflictBadge = document.getElementById('inspect-conflict-status');
    const conflictDesc = document.getElementById('inspect-conflict-desc');
    const hotspotFilesContainer = document.getElementById('inspect-hotspot-files');

    if (pr.hotspots.length > 0) {
        conflictBadge.className = 'px-2 py-0.5 rounded text-[11px] font-semibold bg-rose-950 text-rose-300 border border-rose-800';
        conflictBadge.innerText = `🔥 ${pr.hotspots.length} Hotspot kolizija`;
        conflictDesc.innerText = 'Ovaj PR mijenja datoteke koje istovremeno dira mnogo drugih otvorenih PR-ova. Pazi na merge konflikte!';
        
        hotspotFilesContainer.innerHTML = pr.hotspots.map(h => {
            const competing = state.allPRs.filter(other => other.number !== pr.number && other.files.includes(h)).length;
            return `
                <div class="flex items-center justify-between p-2 rounded bg-rose-950/40 border border-rose-900/60 font-mono text-[11px] text-rose-200">
                    <span class="truncate">${h}</span>
                    <span class="ml-2 px-1.5 py-0.5 rounded bg-rose-800 text-white font-sans text-[10px] whitespace-nowrap">
                        ${competing} konkurentnih PR-ova
                    </span>
                </div>
            `;
        }).join('');
    } else {
        conflictBadge.className = 'px-2 py-0.5 rounded text-[11px] font-semibold bg-emerald-950 text-emerald-300 border border-emerald-800';
        conflictBadge.innerText = `🟢 Čisto (0 konflikata)`;
        conflictDesc.innerText = 'Ovaj PR ne dira nijednu od 46 najkolizijskih datoteka u AnyPS5. Može se spajati bez straha od rušenja drugih PR-ova!';
        hotspotFilesContainer.innerHTML = '';
    }

    // ALAT 2: CLI Commands
    document.getElementById('inspect-cmd-checkout').innerText = `gh pr checkout ${pr.number}`;
    document.getElementById('inspect-cmd-diff').innerText = `gh pr diff ${pr.number}`;
    document.getElementById('inspect-cmd-merge').innerText = `gh pr merge ${pr.number} --squash --delete-branch`;

    // ALAT 3: Checklist
    const chk = pr.checklist || {};
    document.getElementById('chk-build').checked = !!chk.build;
    document.getElementById('chk-types').checked = !!chk.types;
    document.getElementById('chk-locks').checked = !!chk.locks;
    document.getElementById('chk-tests').checked = !!chk.tests;

    // ALAT 4: Notes
    document.getElementById('inspect-notes-field').value = pr.notes || '';

    // ALAT 5: Files List
    document.getElementById('inspect-files-count').innerText = pr.fileCount;
    const filesList = document.getElementById('inspect-files-list');
    filesList.innerHTML = pr.files.map(f => {
        const isHot = pr.hotspots.includes(f);
        return `
            <div class="flex items-center justify-between p-1.5 rounded ${isHot ? 'bg-rose-950/50 border border-rose-800 text-rose-200' : 'bg-slate-900 border border-slate-800 text-slate-300'}">
                <span class="truncate">${escapeHtml(f)}</span>
                ${isHot ? '<span class="text-[10px] text-rose-400 font-sans font-bold">HOTSPOT</span>' : ''}
            </div>
        `;
    }).join('');

    modal.classList.remove('hidden');
    modal.classList.add('flex');
}

function closeInspectModal() {
    const modal = document.getElementById('inspect-modal');
    if (modal) {
        modal.classList.add('hidden');
        modal.classList.remove('flex');
    }
    state.activeInspectPR = null;
}

async function setInspectStatus(newStatus) {
    if (!state.activeInspectPR) return;
    const num = state.activeInspectPR.number;
    state.activeInspectPR.status = newStatus;
    await saveInspectionAndClose();
    showToast(`Postavljen status: ${newStatus}`);
}

function getChecklistState() {
    return {
        build: document.getElementById('chk-build')?.checked || false,
        types: document.getElementById('chk-types')?.checked || false,
        locks: document.getElementById('chk-locks')?.checked || false,
        tests: document.getElementById('chk-tests')?.checked || false
    };
}

async function saveInspectionAndClose() {
    if (!state.activeInspectPR) return;
    const num = state.activeInspectPR.number;
    const notes = document.getElementById('inspect-notes-field').value;
    const checklist = getChecklistState();

    state.activeInspectPR.notes = notes;
    state.activeInspectPR.checklist = checklist;

    closeInspectModal();
    renderMyPRs();
    renderRepoTable();

    if (supabase) {
        await supabase
            .from('prs')
            .update({ 
                status: state.activeInspectPR.status,
                notes: notes,
                checklist: checklist,
                updated_at: new Date().toISOString() 
            })
            .eq('number', num);
    }
    showToast(`Spremljene zabilješke za PR #${num}`);
}

function unassignCurrentPR() {
    if (!state.activeInspectPR) return;
    const num = state.activeInspectPR.number;
    closeInspectModal();
    unassignPR(num);
}


// ==========================================
// VIEW 2: CIJELI REPOZITORIJ (REPO TABLE)
// ==========================================

function setupRepoFilters() {
    const search = document.getElementById('repo-search');
    const catFilter = document.getElementById('repo-cat-filter');
    const riskFilter = document.getElementById('repo-risk-filter');

    const update = () => renderRepoTable();
    if (search) search.addEventListener('input', update);
    if (catFilter) catFilter.addEventListener('change', update);
    if (riskFilter) riskFilter.addEventListener('change', update);
}

function renderRepoTable() {
    const tbody = document.getElementById('repo-table-body');
    if (!tbody) return;

    const q = (document.getElementById('repo-search')?.value || '').toLowerCase().trim();
    const cat = document.getElementById('repo-cat-filter')?.value || 'all';
    const risk = document.getElementById('repo-risk-filter')?.value || 'all';

    let list = state.allPRs.filter(p => {
        if (cat !== 'all' && p.category !== cat) return false;
        if (risk !== 'all' && p.riskLevel !== risk) return false;
        if (q) {
            const matchesNum = p.number.toString().includes(q);
            const matchesTitle = p.title.toLowerCase().includes(q);
            const matchesAuthor = p.author.toLowerCase().includes(q);
            if (!matchesNum && !matchesTitle && !matchesAuthor) return false;
        }
        return true;
    });

    tbody.innerHTML = list.slice(0, 50).map(pr => {
        const isAssignedToMe = pr.assignedReviewer === state.currentMaintainer;
        return `
            <tr class="hover:bg-slate-800/50 transition">
                <td class="px-4 py-2.5 font-mono font-bold text-cyan-400">#${pr.number}</td>
                <td class="px-4 py-2.5 max-w-sm truncate" title="${escapeHtml(pr.title)}">
                    <span class="text-slate-200 font-medium">${escapeHtml(pr.title)}</span>
                    <div class="text-[10px] text-slate-400 font-mono">od @${escapeHtml(pr.author)}</div>
                </td>
                <td class="px-4 py-2.5">${getCategoryBadge(pr.category)}</td>
                <td class="px-4 py-2.5">${getRiskBadge(pr.riskLevel)}</td>
                <td class="px-4 py-2.5 font-mono text-slate-300">
                    ${isAssignedToMe ? '<span class="text-cyan-400 font-bold">👤 Ti (@' + state.currentMaintainer + ')</span>' : (pr.assignedReviewer !== 'none' ? '@' + pr.assignedReviewer : '<span class="text-slate-500">Nitko</span>')}
                </td>
                <td class="px-4 py-2.5 text-right whitespace-nowrap">
                    ${isAssignedToMe ? `
                        <button onclick="openInspectModal(${pr.number})" class="px-2 py-1 bg-cyan-600 text-white rounded font-bold text-[11px]">
                            Ispitaj
                        </button>
                    ` : `
                        <button onclick="assignPR(${pr.number}, state.currentMaintainer)" class="px-2 py-1 bg-slate-800 hover:bg-cyan-600 hover:text-white text-cyan-400 border border-slate-700 rounded text-[11px] font-semibold transition">
                            + Preuzmi
                        </button>
                    `}
                </td>
            </tr>
        `;
    }).join('');
}


// ==========================================
// UTILITY HELPERS & BADGES
// ==========================================

function getRiskBadge(risk) {
    if (risk === 'low') {
        return `<span class="inline-flex items-center gap-1 px-2 py-0.5 rounded-full text-[10px] font-bold bg-emerald-950 text-emerald-300 border border-emerald-800">🟢 Quick Merge</span>`;
    } else if (risk === 'medium') {
        return `<span class="inline-flex items-center gap-1 px-2 py-0.5 rounded-full text-[10px] font-semibold bg-amber-950 text-amber-300 border border-amber-800">🟡 Srednji rizik</span>`;
    } else {
        return `<span class="inline-flex items-center gap-1 px-2 py-0.5 rounded-full text-[10px] font-bold bg-rose-950 text-rose-300 border border-rose-800">🔴 Hotspot rizik</span>`;
    }
}

function getCategoryBadge(cat) {
    return `<span class="px-2 py-0.5 rounded text-[10px] font-semibold bg-slate-800 text-slate-300 border border-slate-700">${escapeHtml(cat)}</span>`;
}

function getStatusBadge(status) {
    const badges = {
        'unassigned': '<span class="text-slate-400 bg-slate-900 border border-slate-800 px-2 py-0.5 rounded text-[10px]">📥 Čeka pregled</span>',
        'in-review': '<span class="text-sky-300 bg-sky-950 border border-sky-800 px-2 py-0.5 rounded text-[10px]">🔍 Pregled u tijeku</span>',
        'approved': '<span class="text-emerald-300 bg-emerald-950 border border-emerald-800 px-2 py-0.5 rounded text-[10px]">✅ Odobreno</span>',
        'changes': '<span class="text-amber-300 bg-amber-950 border border-amber-800 px-2 py-0.5 rounded text-[10px]">⚠️ Traže se izmjene</span>',
        'conflict': '<span class="text-rose-300 bg-rose-950 border border-rose-800 px-2 py-0.5 rounded text-[10px]">🚫 Konflikt</span>',
        'ready': '<span class="text-cyan-300 bg-cyan-950 border border-cyan-800 px-2 py-0.5 rounded text-[10px] font-bold">🚀 Spreman za merge</span>'
    };
    return badges[status] || badges['unassigned'];
}

function copyCliCheckout(prNumber) {
    const cmd = `gh pr checkout ${prNumber}`;
    copyTextToClipboard(cmd, `Kopirano: ${cmd}`);
}

function copyText(elemId, msg) {
    const text = document.getElementById(elemId)?.innerText || '';
    copyTextToClipboard(text, msg);
}

function copyTextToClipboard(text, msg) {
    navigator.clipboard.writeText(text).then(() => {
        showToast(msg);
    }).catch(() => {
        prompt('Kopiraj naredbu:', text);
    });
}

function formatDate(isoStr) {
    if (!isoStr) return '';
    try {
        const d = new Date(isoStr);
        return d.toLocaleDateString('hr-HR', { month: 'short', day: 'numeric', year: 'numeric' });
    } catch (e) {
        return isoStr;
    }
}

function formatRelativeTime(isoStr) {
    if (!isoStr) return '';
    try {
        const diff = (new Date() - new Date(isoStr)) / 1000;
        if (diff < 3600) return `prije ${Math.floor(diff / 60)} min`;
        if (diff < 86400) return `prije ${Math.floor(diff / 3600)} h`;
        return `prije ${Math.floor(diff / 86400)} d`;
    } catch (e) {
        return '';
    }
}

function escapeHtml(str) {
    if (!str) return '';
    return str.replace(/&/g, '&amp;')
              .replace(/</g, '&lt;')
              .replace(/>/g, '&gt;')
              .replace(/"/g, '&quot;')
              .replace(/'/g, '&#039;');
}

function showToast(msg) {
    let toast = document.getElementById('app-toast');
    if (!toast) {
        toast = document.createElement('div');
        toast.id = 'app-toast';
        toast.className = 'fixed bottom-5 right-5 z-50 bg-slate-800 border border-cyan-500/50 text-cyan-200 px-4 py-2.5 rounded-xl shadow-xl text-xs transition-all duration-300 transform translate-y-10 opacity-0';
        document.body.appendChild(toast);
    }
    toast.innerText = msg;
    toast.classList.remove('translate-y-10', 'opacity-0');
    toast.classList.add('translate-y-0', 'opacity-100');

    setTimeout(() => {
        toast.classList.remove('translate-y-0', 'opacity-100');
        toast.classList.add('translate-y-10', 'opacity-0');
    }, 2500);
}
