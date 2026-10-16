/**
 * AnyPS5 Maintainer Workspace - Robust REST & Realtime Supabase Client
 */

const SUPABASE_URL = 'https://nzaxslgwsygpgwqcllpi.supabase.co';
const SUPABASE_ANON_KEY = 'eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.eyJpc3MiOiJzdXBhYmFzZSIsInJlZiI6Im56YXhzbGd3c3lncGd3cWNsbHBpIiwicm9sZSI6ImFub24iLCJpYXQiOjE3OTIxNTU2NjksImV4cCI6MjEwNzczMTY2OX0.G_sibL-5v-wxWagMeY1eAXZXm2b5Bsh0nziN94a4moY';

// Application State
const state = {
    allPRs: [],
    currentMaintainer: 'boykopovar',
    activeView: 'mine', // 'mine' or 'repo'
    myStatusFilter: 'all', // 'all', 'pending', 'approved', 'changes', 'ready'
    selectedClaimPRs: new Set(),
    activeInspectPR: null,
    supabaseConnected: false,
    lastSyncTime: null
};

// Standard REST Headers for Supabase
const supabaseHeaders = {
    'apikey': SUPABASE_ANON_KEY,
    'Authorization': `Bearer ${SUPABASE_ANON_KEY}`,
    'Content-Type': 'application/json'
};

// Initialization on DOMContentLoaded
document.addEventListener('DOMContentLoaded', () => {
    loadPersistentUser();
    loadLocalFallbackData();
    setupMaintainerSelector();
    setupRepoFilters();
    renderMyPRs();
    renderRepoTable();

    // 1. Initial direct REST fetch from Supabase
    refreshDataFromSupabase();

    // 2. Setup Realtime WebSockets if Supabase JS SDK loaded
    setupSupabaseRealtime();

    // 3. Periodic 60-second auto-sync
    setInterval(() => {
        refreshDataFromSupabase(false);
    }, 60000);
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

// 0ms instant initial render from bundled fallback
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

// ==========================================
// SUPABASE REST COMMUNICATION (BULLETPROOF)
// ==========================================

// Direct REST Fetch: works 100% reliably in any browser
async function refreshDataFromSupabase(showToastAlert = false) {
    const statusText = document.getElementById('supabase-status-text');
    const pill = document.getElementById('supabase-status-pill');

    try {
        if (statusText) statusText.innerText = 'Osvježavanje...';

        const res = await fetch(`${SUPABASE_URL}/rest/v1/prs?select=*&order=number.desc`, {
            headers: supabaseHeaders
        });

        if (!res.ok) {
            throw new Error(`HTTP ${res.status}: ${res.statusText}`);
        }

        const data = await res.json();
        if (Array.isArray(data) && data.length > 0) {
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
                category: row.category || 'PRX High-Level Modules',
                hotspots: row.hotspots || [],
                riskLevel: row.risk_level || 'low',
                size: row.size || 'S',
                isDraft: row.is_draft || false,
                assignedReviewer: row.assigned_reviewer || 'none',
                status: row.status || 'unassigned',
                notes: row.notes || '',
                checklist: row.checklist || {}
            }));

            state.supabaseConnected = true;
            state.lastSyncTime = new Date();

            const timeStr = state.lastSyncTime.toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' });
            if (statusText) statusText.innerText = `🟢 ${data.length} PR (${timeStr})`;
            if (pill) {
                pill.className = "text-[10px] px-2.5 py-1 rounded-full bg-emerald-950/80 text-emerald-300 border border-emerald-800 font-mono flex items-center gap-1.5 transition";
            }

            renderMyPRs();
            renderRepoTable();

            if (showToastAlert) {
                showToast(`Sinkronizirano sa Supabaseom (${data.length} PR-ova)`);
            }
        }
    } catch (err) {
        console.error('Supabase REST sync error:', err);
        if (statusText) statusText.innerText = '⚠️ Greška veze';
        if (pill) {
            pill.className = "text-[10px] px-2.5 py-1 rounded-full bg-rose-950 text-rose-300 border border-rose-800 font-mono flex items-center gap-1.5 transition";
        }
    }
}

// Direct REST PATCH: updates single or batch PRs in Supabase
async function updatePRInSupabase(number, fields) {
    try {
        const payload = {
            ...fields,
            updated_at: new Date().toISOString()
        };

        const res = await fetch(`${SUPABASE_URL}/rest/v1/prs?number=eq.${number}`, {
            method: 'PATCH',
            headers: {
                ...supabaseHeaders,
                'Prefer': 'return=minimal'
            },
            body: JSON.stringify(payload)
        });

        if (!res.ok) {
            console.error('Supabase PATCH error:', res.status, res.statusText);
        }
    } catch (e) {
        console.error('Supabase update failed:', e);
    }
}

// Batch PATCH for multiple PRs
async function updateBatchPRsInSupabase(numbers, fields) {
    if (!numbers || numbers.length === 0) return;
    try {
        const filterStr = numbers.map(n => `number.eq.${n}`).join(',');
        const payload = {
            ...fields,
            updated_at: new Date().toISOString()
        };

        await fetch(`${SUPABASE_URL}/rest/v1/prs?number=in.(${numbers.join(',')})`, {
            method: 'PATCH',
            headers: {
                ...supabaseHeaders,
                'Prefer': 'return=minimal'
            },
            body: JSON.stringify(payload)
        });
    } catch (e) {
        console.error('Batch Supabase update failed:', e);
    }
}

// Realtime WebSocket setup
function setupSupabaseRealtime() {
    if (!window.supabase || typeof window.supabase.createClient !== 'function') {
        console.log('Supabase JS SDK not loaded, running in pure REST mode');
        return;
    }

    try {
        const sbClient = window.supabase.createClient(SUPABASE_URL, SUPABASE_ANON_KEY);
        sbClient
            .channel('public:prs_channel')
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
    } catch (err) {
        console.warn('Realtime subscription error (using REST polling instead):', err);
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

    // Apply active status filter
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
            <div class="p-8 text-center bg-slate-900/60 rounded-xl border border-slate-800 text-slate-400 text-xs">
                Nema PR-ova s ovim statusom. <button onclick="resetMyStatusFilter()" class="text-cyan-400 underline ml-1 cursor-pointer">Prikaži sve</button>
            </div>
        `;
        return;
    }

    container.innerHTML = visiblePRs.map(pr => {
        const riskBadge = getRiskBadge(pr.riskLevel);
        const catBadge = getCategoryBadge(pr.category);
        const statusBadge = getStatusBadge(pr.status);
        const hasHotspots = pr.hotspots && pr.hotspots.length > 0;

        return `
            <div class="p-3.5 sm:p-4 rounded-xl bg-cardbg border border-slate-800/90 hover:border-slate-700 transition shadow-sm space-y-2.5">
                <div class="flex items-center justify-between gap-2 flex-wrap">
                    <div class="flex items-center gap-2 flex-wrap">
                        <span class="font-mono font-bold text-cyan-400 text-xs sm:text-sm">#${pr.number}</span>
                        ${catBadge}
                        ${riskBadge}
                        ${statusBadge}
                    </div>
                    <div class="text-[11px] text-slate-400 font-mono">
                        <span class="text-slate-300 font-semibold">${pr.fileCount} ${pr.fileCount === 1 ? 'datoteka' : 'datoteka'}</span> 
                        (<span class="text-emerald-400">+${pr.additions}</span> / <span class="text-rose-400">-${pr.deletions}</span>)
                    </div>
                </div>

                <div>
                    <h3 class="text-sm sm:text-base font-semibold text-slate-100 hover:text-cyan-300 transition cursor-pointer" onclick="openInspectModal(${pr.number})">
                        ${escapeHtml(pr.title)}
                    </h3>
                    <div class="flex items-center gap-2 mt-1 text-[11px] text-slate-400 flex-wrap">
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
                        <span class="text-slate-500">📝:</span>
                        <span class="italic text-slate-300 truncate">${escapeHtml(pr.notes)}</span>
                    </div>
                ` : ''}

                <!-- Action Tools Row -->
                <div class="pt-2 border-t border-slate-800 flex items-center justify-between flex-wrap gap-2">
                    <div class="flex items-center gap-1.5">
                        <button onclick="openInspectModal(${pr.number})" 
                                class="px-3 py-1.5 rounded-lg bg-cyan-600 hover:bg-cyan-500 text-white font-bold text-xs shadow-md shadow-cyan-600/20 transition flex items-center gap-1 cursor-pointer">
                            <span>🧪</span> Ispitaj i alati
                        </button>
                        <button onclick="copyCliCheckout(${pr.number})" 
                                class="px-2.5 py-1.5 rounded-lg bg-slate-800 hover:bg-slate-700 text-slate-300 text-xs font-mono transition border border-slate-700 flex items-center gap-1 cursor-pointer">
                            <span>💻</span> gh checkout
                        </button>
                    </div>

                    <div class="flex items-center gap-1.5">
                        <select onchange="quickUpdateMyStatus(${pr.number}, this.value)" 
                                class="bg-slate-900 border border-slate-700 text-slate-200 text-xs rounded-lg px-2 py-1.5 focus:outline-none focus:border-cyan-500 cursor-pointer">
                            <option value="in-review" ${pr.status === 'in-review' || pr.status === 'unassigned' ? 'selected' : ''}>🔍 Čeka pregled</option>
                            <option value="approved" ${pr.status === 'approved' ? 'selected' : ''}>✅ Odobreno</option>
                            <option value="changes" ${pr.status === 'changes' ? 'selected' : ''}>⚠️ Zatraži izmjene</option>
                            <option value="conflict" ${pr.status === 'conflict' ? 'selected' : ''}>🚫 Konflikt</option>
                            <option value="ready" ${pr.status === 'ready' ? 'selected' : ''}>🚀 Za merge</option>
                        </select>
                        <button onclick="unassignPR(${pr.number})" title="Ukloni iz mog reda" 
                                class="p-1.5 rounded-lg text-slate-500 hover:text-rose-400 hover:bg-slate-800 transition text-xs cursor-pointer">
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

function filterMyPRs(status, element) {
    state.myStatusFilter = status;
    document.querySelectorAll('.my-pill').forEach(btn => {
        btn.classList.remove('bg-slate-800', 'border-cyan-500/50', 'text-cyan-300');
        btn.classList.add('bg-slate-900', 'border-slate-800', 'text-slate-400');
    });
    if (element) {
        element.classList.add('bg-slate-800', 'border-cyan-500/50', 'text-cyan-300');
        element.classList.remove('bg-slate-900', 'border-slate-800', 'text-slate-400');
    }
    renderMyPRs();
}

function resetMyStatusFilter() {
    const firstPill = document.querySelector('.my-pill');
    filterMyPRs('all', firstPill);
}

async function quickUpdateMyStatus(prNumber, newStatus) {
    const target = state.allPRs.find(p => p.number === prNumber);
    if (target) target.status = newStatus;
    renderMyPRs();

    await updatePRInSupabase(prNumber, { status: newStatus });
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

    await updatePRInSupabase(prNumber, { 
        assigned_reviewer: maintainerId, 
        status: newStatus 
    });
}

async function unassignPR(prNumber) {
    const target = state.allPRs.find(p => p.number === prNumber);
    if (target) {
        target.assignedReviewer = 'none';
        target.status = 'unassigned';
    }

    renderMyPRs();
    renderRepoTable();

    await updatePRInSupabase(prNumber, { 
        assigned_reviewer: 'none', 
        status: 'unassigned' 
    });
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

    // Show available unassigned PRs
    const available = state.allPRs.filter(p => p.assignedReviewer !== state.currentMaintainer).slice(0, 50);

    container.innerHTML = available.map(pr => {
        const isChecked = state.selectedClaimPRs.has(pr.number);
        return `
            <div class="flex items-center justify-between p-2 rounded-lg bg-slate-900 border border-slate-800 hover:border-slate-700 transition">
                <div class="flex items-center gap-2 flex-1 min-w-0 pr-2">
                    <input type="checkbox" ${isChecked ? 'checked' : ''} 
                           onchange="toggleClaimSelect(${pr.number}, this.checked)"
                           class="rounded bg-slate-800 border-slate-700 text-cyan-500 focus:ring-cyan-500 cursor-pointer">
                    <span class="font-mono font-bold text-cyan-400 text-xs">#${pr.number}</span>
                    <span class="text-xs text-slate-200 truncate" title="${escapeHtml(pr.title)}">${escapeHtml(pr.title)}</span>
                </div>
                <div class="flex items-center gap-2">
                    ${getRiskBadge(pr.riskLevel)}
                    <button onclick="quickClaimSingle(${pr.number})" 
                            class="px-2 py-1 bg-cyan-950 hover:bg-cyan-800 text-cyan-300 border border-cyan-800 rounded text-[11px] font-semibold transition cursor-pointer">
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

    await updateBatchPRsInSupabase(selected, {
        assigned_reviewer: state.currentMaintainer,
        status: 'in-review'
    });
    showToast(`Dodano ${count} PR-ova u tvoj radni red!`);
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

    await updateBatchPRsInSupabase(numbers, {
        assigned_reviewer: state.currentMaintainer,
        status: 'in-review'
    });
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

    if (pr.hotspots && pr.hotspots.length > 0) {
        conflictBadge.className = 'px-2 py-0.5 rounded text-[10px] font-semibold bg-rose-950 text-rose-300 border border-rose-800';
        conflictBadge.innerText = `🔥 ${pr.hotspots.length} Hotspot kolizija`;
        conflictDesc.innerText = 'Ovaj PR mijenja datoteke koje istovremeno dira mnogo drugih otvorenih PR-ova. Pazi na merge konflikte!';
        
        hotspotFilesContainer.innerHTML = pr.hotspots.map(h => {
            const competing = state.allPRs.filter(other => other.number !== pr.number && other.files && other.files.includes(h)).length;
            return `
                <div class="flex items-center justify-between p-2 rounded bg-rose-950/40 border border-rose-900/60 font-mono text-[11px] text-rose-200">
                    <span class="truncate">${escapeHtml(h)}</span>
                    <span class="ml-2 px-1.5 py-0.5 rounded bg-rose-800 text-white font-sans text-[10px] whitespace-nowrap">
                        ${competing} konkurentnih PR-ova
                    </span>
                </div>
            `;
        }).join('');
    } else {
        conflictBadge.className = 'px-2 py-0.5 rounded text-[10px] font-semibold bg-emerald-950 text-emerald-300 border border-emerald-800';
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
    filesList.innerHTML = (pr.files || []).map(f => {
        const isHot = pr.hotspots && pr.hotspots.includes(f);
        return `
            <div class="flex items-center justify-between p-1.5 rounded ${isHot ? 'bg-rose-950/50 border border-rose-800 text-rose-200' : 'bg-slate-900 border border-slate-800 text-slate-300'}">
                <span class="truncate">${escapeHtml(f)}</span>
                ${isHot ? '<span class="text-[10px] text-rose-400 font-sans font-bold ml-1">HOTSPOT</span>' : ''}
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
    showToast(`Status: ${newStatus}`);
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

    await updatePRInSupabase(num, {
        status: state.activeInspectPR.status,
        notes: notes,
        checklist: checklist
    });
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
    const countTitle = document.getElementById('repo-count-title');
    const repoTotalBadge = document.getElementById('repo-total-count-badge');
    if (!tbody) return;

    if (countTitle) countTitle.innerText = state.allPRs.length;
    if (repoTotalBadge) repoTotalBadge.innerText = state.allPRs.length;

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
                <td class="px-4 py-2.5 whitespace-nowrap">${getCategoryBadge(pr.category)}</td>
                <td class="px-4 py-2.5 whitespace-nowrap">${getRiskBadge(pr.riskLevel)}</td>
                <td class="px-4 py-2.5 font-mono text-slate-300 whitespace-nowrap">
                    ${isAssignedToMe ? '<span class="text-cyan-400 font-bold">👤 Ti (@' + state.currentMaintainer + ')</span>' : (pr.assignedReviewer !== 'none' ? '@' + pr.assignedReviewer : '<span class="text-slate-500">Nitko</span>')}
                </td>
                <td class="px-4 py-2.5 text-right whitespace-nowrap">
                    ${isAssignedToMe ? `
                        <button onclick="openInspectModal(${pr.number})" class="px-2 py-1 bg-cyan-600 text-white rounded font-bold text-[11px] cursor-pointer">
                            Ispitaj
                        </button>
                    ` : `
                        <button onclick="assignPR(${pr.number}, state.currentMaintainer)" class="px-2 py-1 bg-slate-800 hover:bg-cyan-600 hover:text-white text-cyan-400 border border-slate-700 rounded text-[11px] font-semibold transition cursor-pointer">
                            + Preuzmi
                        </button>
                    `}
                </td>
            </tr>
        `;
    }).join('');
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
// UTILITY HELPERS & BADGES
// ==========================================

function getRiskBadge(risk) {
    if (risk === 'low') {
        return `<span class="inline-flex items-center gap-1 px-1.5 py-0.5 rounded-full text-[10px] font-bold bg-emerald-950 text-emerald-300 border border-emerald-800 whitespace-nowrap">🟢 Quick Merge</span>`;
    } else if (risk === 'medium') {
        return `<span class="inline-flex items-center gap-1 px-1.5 py-0.5 rounded-full text-[10px] font-semibold bg-amber-950 text-amber-300 border border-amber-800 whitespace-nowrap">🟡 Srednji rizik</span>`;
    } else {
        return `<span class="inline-flex items-center gap-1 px-1.5 py-0.5 rounded-full text-[10px] font-bold bg-rose-950 text-rose-300 border border-rose-800 whitespace-nowrap">🔴 Hotspot rizik</span>`;
    }
}

function getCategoryBadge(cat) {
    return `<span class="px-1.5 py-0.5 rounded text-[10px] font-semibold bg-slate-800 text-slate-300 border border-slate-700 whitespace-nowrap">${escapeHtml(cat)}</span>`;
}

function getStatusBadge(status) {
    const badges = {
        'unassigned': '<span class="text-slate-400 bg-slate-900 border border-slate-800 px-1.5 py-0.5 rounded text-[10px] whitespace-nowrap">📥 Čeka pregled</span>',
        'in-review': '<span class="text-sky-300 bg-sky-950 border border-sky-800 px-1.5 py-0.5 rounded text-[10px] whitespace-nowrap">🔍 Pregled u tijeku</span>',
        'approved': '<span class="text-emerald-300 bg-emerald-950 border border-emerald-800 px-1.5 py-0.5 rounded text-[10px] whitespace-nowrap">✅ Odobreno</span>',
        'changes': '<span class="text-amber-300 bg-amber-950 border border-amber-800 px-1.5 py-0.5 rounded text-[10px] whitespace-nowrap">⚠️ Traže se izmjene</span>',
        'conflict': '<span class="text-rose-300 bg-rose-950 border border-rose-800 px-1.5 py-0.5 rounded text-[10px] whitespace-nowrap">🚫 Konflikt</span>',
        'ready': '<span class="text-cyan-300 bg-cyan-950 border border-cyan-800 px-1.5 py-0.5 rounded text-[10px] font-bold whitespace-nowrap">🚀 Za merge</span>'
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
    if (navigator.clipboard && navigator.clipboard.writeText) {
        navigator.clipboard.writeText(text).then(() => {
            showToast(msg);
        }).catch(() => {
            prompt('Kopiraj naredbu:', text);
        });
    } else {
        prompt('Kopiraj naredbu:', text);
    }
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
        toast.className = 'fixed bottom-4 right-4 z-50 bg-slate-800 border border-cyan-500/50 text-cyan-200 px-3.5 py-2 rounded-xl shadow-2xl text-xs transition-all duration-300 transform translate-y-10 opacity-0 pointer-events-none max-w-xs';
        document.body.appendChild(toast);
    }
    toast.innerText = msg;
    toast.classList.remove('translate-y-10', 'opacity-0');
    toast.classList.add('translate-y-0', 'opacity-100');

    setTimeout(() => {
        toast.classList.remove('translate-y-0', 'opacity-100');
        toast.classList.add('translate-y-10', 'opacity-0');
    }, 2400);
}
