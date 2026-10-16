/**
 * AnyPS5 Maintainer Workspace - Supabase Server-Side Minibatches & Collision Prevention
 */

const SUPABASE_URL = 'https://nzaxslgwsygpgwqcllpi.supabase.co';
const SUPABASE_ANON_KEY = 'eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.eyJpc3MiOiJzdXBhYmFzZSIsInJlZiI6Im56YXhzbGd3c3lncGd3cWNsbHBpIiwicm9sZSI6ImFub24iLCJpYXQiOjE3OTIxNTU2NjksImV4cCI6MjEwNzczMTY2OX0.G_sibL-5v-wxWagMeY1eAXZXm2b5Bsh0nziN94a4moY';

// Recognized Official AnyPS5 Repository Maintainers
const KNOWN_MAINTAINERS = [
    'boykopovar',
    'ivica00',
    'Honza11',
    'Eljack',
    'maccer_acer',
    'JJenkx',
    'MarcZX100',
    'DavidSamir'
];

// Application State
const state = {
    allPRs: [],
    minibatches: [],
    batchViewMode: 'compact', // 'compact' (default, highly dense for 50+ batches) or 'grid'
    batchSearchTerm: '',
    batchLimit: 50,
    expandedBatches: new Set(),
    activeBatchForModal: null,
    activeBatchModalTab: 'ai-prompt', // 'ai-prompt', 'merge', or 'test'
    currentUser: {
        username: 'boykopovar',
        name: 'Boyko Povar',
        avatar: 'https://github.com/boykopovar.png',
        role: 'maintainer', // 'maintainer' or 'reviewer'
        token: ''
    },
    reviewerWatchlist: new Set(),
    activeReviewerRecommendation: 'approve',
    currentMaintainer: 'boykopovar',
    activeView: 'mine', // 'mine', 'batches', or 'repo'
    myStatusFilter: 'all', // 'all', 'pending', 'approved', 'changes', 'ready'
    selectedClaimPRs: new Set(),
    activeInspectPR: null,
    supabaseConnected: false,
    lastSyncTime: null
};

// Role Check Helper
function isCurrentUserMaintainer() {
    return state.currentUser && state.currentUser.role === 'maintainer';
}

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
    initSupabaseAuth();
    updateUserUI();
    setupRepoFilters();
    renderMyPRs();
    renderRepoTable();
    renderSidebarStats();

    // 1. Initial direct REST fetch from Supabase (PRs & Batches)
    refreshDataFromSupabase();

    // 2. Setup Realtime WebSockets for both PRs and Batches
    setupSupabaseRealtime();

    // 3. Periodic 60-second auto-sync
    setInterval(() => {
        refreshDataFromSupabase(false);
    }, 60000);
});

// Load persistent maintainer identity
function loadPersistentUser() {
    try {
        const savedAuth = localStorage.getItem('anyps5_auth_user');
        if (savedAuth) {
            state.currentUser = JSON.parse(savedAuth);
            state.currentMaintainer = state.currentUser?.username || '';
        } else {
            state.currentUser = null;
            state.currentMaintainer = '';
        }
        const savedWatchlist = localStorage.getItem('anyps5_reviewer_watchlist');
        if (savedWatchlist) {
            state.reviewerWatchlist = new Set(JSON.parse(savedWatchlist));
        }
    } catch (e) {
        console.warn('LocalStorage error', e);
        state.currentUser = null;
        state.currentMaintainer = '';
    }
}

function savePersistentUser() {
    try {
        if (state.currentUser) {
            localStorage.setItem('anyps5_auth_user', JSON.stringify(state.currentUser));
            localStorage.setItem('anyps5_current_maintainer', state.currentUser.username);
        } else {
            localStorage.removeItem('anyps5_auth_user');
            localStorage.removeItem('anyps5_current_maintainer');
        }
        localStorage.setItem('anyps5_reviewer_watchlist', JSON.stringify(Array.from(state.reviewerWatchlist)));
    } catch (e) {
        console.warn('LocalStorage error', e);
    }
}

function updateUserUI() {
    const u = state.currentUser;
    const isM = isCurrentUserMaintainer();
    const isLoggedIn = !!u;

    // Header elements
    const avatarEl = document.getElementById('header-user-avatar');
    const userEl = document.getElementById('header-username');
    const roleBadgeEl = document.getElementById('header-role-badge');
    const headerInfo = document.getElementById('header-user-info');
    const headerBadgeWrap = document.getElementById('header-user-badge-wrap');
    const headerLoginPrompt = document.getElementById('header-login-prompt');

    if (isLoggedIn) {
        if (headerInfo) headerInfo.classList.remove('hidden');
        if (headerBadgeWrap) headerBadgeWrap.classList.remove('hidden');
        if (headerLoginPrompt) headerLoginPrompt.classList.add('hidden');

        if (avatarEl) avatarEl.src = u.avatar || `https://github.com/${u.username}.png`;
        if (userEl) userEl.innerText = `@${u.username}`;
        if (roleBadgeEl) {
            if (isM) {
                roleBadgeEl.className = 'px-1.5 py-0.2 rounded text-[10px] font-bold bg-emerald-950 text-emerald-300 border border-emerald-800 whitespace-nowrap';
                roleBadgeEl.innerText = '🛡️ Maintainer';
            } else {
                roleBadgeEl.className = 'px-1.5 py-0.2 rounded text-[10px] font-bold bg-purple-950 text-purple-300 border border-purple-800 whitespace-nowrap';
                roleBadgeEl.innerText = '🔍 Community Reviewer';
            }
        }
    } else {
        if (headerInfo) headerInfo.classList.add('hidden');
        if (headerBadgeWrap) headerBadgeWrap.classList.add('hidden');
        if (headerLoginPrompt) headerLoginPrompt.classList.remove('hidden');
    }

    // Modal state (logged out vs logged in)
    const loggedOutState = document.getElementById('oauth-logged-out-state');
    const loggedInState = document.getElementById('oauth-logged-in-state');
    if (loggedOutState && loggedInState) {
        if (isLoggedIn) {
            loggedOutState.classList.add('hidden');
            loggedInState.classList.remove('hidden');

            const oauthAvatar = document.getElementById('oauth-user-avatar');
            const oauthUser = document.getElementById('oauth-username');
            const oauthFull = document.getElementById('oauth-fullname');
            const oauthRole = document.getElementById('oauth-role-badge');

            if (oauthAvatar) oauthAvatar.src = u.avatar || `https://github.com/${u.username}.png`;
            if (oauthUser) oauthUser.innerText = `@${u.username}`;
            if (oauthFull) oauthFull.innerText = u.name || u.username;
            if (oauthRole) {
                if (isM) {
                    oauthRole.className = 'px-1.5 py-0.2 rounded text-[10px] font-bold bg-emerald-950 text-emerald-300 border border-emerald-800';
                    oauthRole.innerText = '🛡️ Maintainer';
                } else {
                    oauthRole.className = 'px-1.5 py-0.2 rounded text-[10px] font-bold bg-purple-950 text-purple-300 border border-purple-800';
                    oauthRole.innerText = '🔍 Community Reviewer';
                }
            }
        } else {
            loggedOutState.classList.remove('hidden');
            loggedInState.classList.add('hidden');
        }
    }

    // Tab 1 (Mine vs Review Workbench)
    const tabMineIcon = document.getElementById('tab-mine-icon');
    const tabMineTitle = document.getElementById('tab-mine-title');
    if (tabMineIcon) tabMineIcon.innerText = isM ? '📦' : '🔍';
    if (tabMineTitle) tabMineTitle.innerText = isM ? 'My Batches' : 'My Batch Reviews';

    // Hero Banner
    const heroName = document.getElementById('hero-maintainer-name');
    const heroRole = document.getElementById('hero-role-badge');
    const heroDesc = document.getElementById('hero-maintainer-desc');
    const heroBtnText = document.getElementById('hero-action-btn-text');

    if (heroName) heroName.innerText = isLoggedIn ? `@${u.username}` : 'AnyPS5 Guest';
    if (heroRole) {
        if (isM) {
            heroRole.className = 'px-2 py-0.5 rounded-full text-[10px] font-bold bg-emerald-950 text-emerald-300 border border-emerald-800';
            heroRole.innerText = '🛡️ Official Maintainer';
        } else if (isLoggedIn) {
            heroRole.className = 'px-2 py-0.5 rounded-full text-[10px] font-bold bg-purple-950 text-purple-300 border border-purple-800';
            heroRole.innerText = '🔍 Community Reviewer';
        } else {
            heroRole.className = 'px-2 py-0.5 rounded-full text-[10px] font-bold bg-slate-800 text-slate-300 border border-slate-700';
            heroRole.innerText = '👋 Guest (GitHub Login Required)';
        }
    }
    if (heroDesc) {
        heroDesc.innerText = isM 
            ? 'Conflict-free minibatches assigned to you for review, testing, and merge. Synced with Supabase in real-time.'
            : (isLoggedIn 
                ? 'As a community contributor, explore conflict-free batches, test PRs locally, and generate code reviews.'
                : 'Sign in with your GitHub account to claim batches, resolve conflicts, and post reviews.');
    }
    if (heroBtnText) {
        heroBtnText.innerText = isM ? 'Explore Minibatch Studio' : (isLoggedIn ? 'Explore Batches for Testing' : 'Sign in with GitHub');
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
// SUPABASE REST COMMUNICATION (PRs & BATCHES)
// ==========================================

async function refreshDataFromSupabase(showToastAlert = false) {
    const statusText = document.getElementById('supabase-status-text');
    const pill = document.getElementById('supabase-status-pill');

    try {
        if (statusText) statusText.innerText = 'Refreshing...';

        // 1. Fetch all PRs
        const prsRes = await fetch(`${SUPABASE_URL}/rest/v1/prs?select=*&order=number.desc`, {
            headers: supabaseHeaders
        });

        // 2. Fetch all Server-Side Minibatches
        const batchesRes = await fetch(`${SUPABASE_URL}/rest/v1/batches?select=*&order=id.asc`, {
            headers: supabaseHeaders
        });

        if (!prsRes.ok) throw new Error(`PRs fetch failed: ${prsRes.status}`);

        const prData = await prsRes.json();
        if (Array.isArray(prData) && prData.length > 0) {
            state.allPRs = prData.map(row => ({
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
        }

        // Parse Server-Side Batches
        if (batchesRes.ok) {
            const batchData = await batchesRes.json();
            if (Array.isArray(batchData)) {
                state.minibatches = batchData.map(b => {
                    const prList = (b.pr_numbers || []).map(num => state.allPRs.find(p => p.number === num)).filter(Boolean);
                    return {
                        id: b.id,
                        title: b.title,
                        category: b.category,
                        complexity: b.complexity,
                        complexityLabel: b.complexity_label,
                        prCount: b.pr_count,
                        prNumbers: b.pr_numbers || [],
                        status: b.status || 'available',
                        claimedBy: b.claimed_by || 'none',
                        claimedAt: b.claimed_at,
                        prs: prList,
                        selectedPRs: new Set(b.pr_numbers || [])
                    };
                });
            }
        }

        state.supabaseConnected = true;
        state.lastSyncTime = new Date();

        const timeStr = state.lastSyncTime.toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' });
        if (statusText) statusText.innerText = `🟢 ${state.allPRs.length} PR (${timeStr})`;
        if (pill) {
            pill.className = "text-[10px] px-2.5 py-1 rounded-full bg-emerald-950/80 text-emerald-300 border border-emerald-800 font-mono flex items-center gap-1.5 transition";
        }

        renderMyPRs();
        renderMinibatches();
        renderRepoTable();
        renderSidebarStats();

        if (showToastAlert) {
            showToast(`Synced with Supabase (${state.minibatches.length} batches)`);
        }
    } catch (err) {
        console.error('Supabase REST sync error:', err);
        if (statusText) statusText.innerText = '⚠️ Connection error';
        if (pill) {
            pill.className = "text-[10px] px-2.5 py-1 rounded-full bg-rose-950 text-rose-300 border border-rose-800 font-mono flex items-center gap-1.5 transition";
        }
    }
}

async function updatePRInSupabase(number, fields) {
    try {
        await fetch(`${SUPABASE_URL}/rest/v1/prs?number=eq.${number}`, {
            method: 'PATCH',
            headers: {
                ...supabaseHeaders,
                'Prefer': 'return=minimal'
            },
            body: JSON.stringify({
                ...fields,
                updated_at: new Date().toISOString()
            })
        });
    } catch (e) {
        console.error('Supabase update failed:', e);
    }
}

async function updateBatchPRsInSupabase(numbers, fields) {
    if (!numbers || numbers.length === 0) return;
    try {
        await fetch(`${SUPABASE_URL}/rest/v1/prs?number=in.(${numbers.join(',')})`, {
            method: 'PATCH',
            headers: {
                ...supabaseHeaders,
                'Prefer': 'return=minimal'
            },
            body: JSON.stringify({
                ...fields,
                updated_at: new Date().toISOString()
            })
        });
    } catch (e) {
        console.error('Batch Supabase update failed:', e);
    }
}

// Setup Realtime WebSocket for both PRs and Batches
function setupSupabaseRealtime() {
    if (!window.supabase || typeof window.supabase.createClient !== 'function') {
        return;
    }

    try {
        const sbClient = getSupabaseClient();
        if (!sbClient) return;
        
        // 1. Listen for PR changes
        sbClient
            .channel('public:live_prs')
            .on('postgres_changes', { event: '*', schema: 'public', table: 'prs' }, (payload) => {
                const updatedRow = payload.new;
                if (!updatedRow || !updatedRow.number) return;

                const existingPR = state.allPRs.find(p => p.number === updatedRow.number);
                if (existingPR) {
                    existingPR.assignedReviewer = updatedRow.assigned_reviewer || 'none';
                    existingPR.status = updatedRow.status || 'unassigned';
                    existingPR.notes = updatedRow.notes || '';
                    existingPR.checklist = updatedRow.checklist || {};

                    renderMyPRs();
                    renderRepoTable();
                }
            })
            .subscribe();

        // 2. Listen for Server-Side Minibatch locks and claims
        sbClient
            .channel('public:live_batches')
            .on('postgres_changes', { event: '*', schema: 'public', table: 'batches' }, (payload) => {
                const updatedBatch = payload.new;
                if (!updatedBatch || !updatedBatch.id) return;

                const existingBatch = state.minibatches.find(b => b.id === updatedBatch.id);
                if (existingBatch) {
                    const wasClaimedByMe = existingBatch.claimedBy === state.currentMaintainer;
                    const isNowClaimedByOther = updatedBatch.claimed_by && updatedBatch.claimed_by !== 'none' && updatedBatch.claimed_by !== state.currentMaintainer;

                    existingBatch.status = updatedBatch.status || 'available';
                    existingBatch.claimedBy = updatedBatch.claimed_by || 'none';
                    existingBatch.claimedAt = updatedBatch.claimed_at;

                    renderMinibatches();

                    if (!wasClaimedByMe && isNowClaimedByOther) {
                        showToast(`🔒 @${updatedBatch.claimed_by} claimed batch "${existingBatch.title}"`);
                    }
                }
            })
            .subscribe();

    } catch (err) {
        console.warn('Realtime subscription error:', err);
    }
}


// ==========================================
// MINIBATCH STUDIO (SERVER-SIDE LOCKING)
// ==========================================

// ==========================================
// MINIBATCH STUDIO (SERVER-SIDE LOCKING & HIGH-DENSITY VIEW)
// ==========================================

function setBatchViewMode(mode) {
    state.batchViewMode = mode;
    const btnCompact = document.getElementById('btn-batch-view-compact');
    const btnGrid = document.getElementById('btn-batch-view-grid');
    const container = document.getElementById('minibatches-container');

    if (btnCompact && btnGrid) {
        if (mode === 'compact') {
            btnCompact.className = 'px-2.5 py-1 rounded text-xs font-semibold transition text-cyan-300 bg-slate-800 shadow-sm';
            btnGrid.className = 'px-2.5 py-1 rounded text-xs font-medium transition text-slate-400 hover:text-slate-200';
            if (container) container.className = 'space-y-1.5';
        } else {
            btnGrid.className = 'px-2.5 py-1 rounded text-xs font-semibold transition text-cyan-300 bg-slate-800 shadow-sm';
            btnCompact.className = 'px-2.5 py-1 rounded text-xs font-medium transition text-slate-400 hover:text-slate-200';
            if (container) container.className = 'grid grid-cols-1 lg:grid-cols-2 gap-4';
        }
    }
    renderMinibatches();
}

function toggleBatchExpand(batchId) {
    if (state.expandedBatches.has(batchId)) {
        state.expandedBatches.delete(batchId);
    } else {
        state.expandedBatches.add(batchId);
    }
    renderMinibatches();
}

function toggleAllBatchesExpand() {
    const btn = document.getElementById('btn-toggle-all-batches');
    const visibleBatches = getFilteredBatches();
    const allExpanded = visibleBatches.length > 0 && visibleBatches.every(b => state.expandedBatches.has(b.id));

    if (allExpanded) {
        visibleBatches.forEach(b => state.expandedBatches.delete(b.id));
        if (btn) btn.innerText = '↕ Expand All';
    } else {
        visibleBatches.forEach(b => state.expandedBatches.add(b.id));
        if (btn) btn.innerText = '↕ Collapse All';
    }
    renderMinibatches();
}

function getFilteredBatches() {
    const searchTerm = (document.getElementById('batch-search-input')?.value || '').trim().toLowerCase();
    const catFilter = document.getElementById('batch-cat-filter')?.value || 'all';
    const tierFilter = document.getElementById('batch-tier-filter')?.value || 'all';
    const statusFilter = document.getElementById('batch-status-filter')?.value || 'available';

    return state.minibatches.filter(b => {
        if (catFilter !== 'all' && b.category !== catFilter) return false;
        if (tierFilter !== 'all' && b.complexity !== tierFilter) return false;

        if (statusFilter === 'available') {
            if (!(b.status === 'available' || b.claimedBy === 'none')) return false;
        } else if (statusFilter === 'mine') {
            if (b.claimedBy !== state.currentMaintainer) return false;
        } else if (statusFilter === 'claimed') {
            if (!(b.claimedBy && b.claimedBy !== 'none' && b.claimedBy !== state.currentMaintainer)) return false;
        }

        if (searchTerm) {
            const inTitle = (b.title || '').toLowerCase().includes(searchTerm);
            const inCat = (b.category || '').toLowerCase().includes(searchTerm);
            const inId = (b.id || '').toLowerCase().includes(searchTerm);
            const inPRs = (b.prNumbers || []).some(num => num.toString().includes(searchTerm));
            const inPRTitles = (b.prs || []).some(p => (p.title || '').toLowerCase().includes(searchTerm));
            if (!inTitle && !inCat && !inId && !inPRs && !inPRTitles) return false;
        }

        return true;
    });
}

function renderMinibatches() {
    const container = document.getElementById('minibatches-container');
    const visibleCountEl = document.getElementById('batches-visible-count');
    const badgeEl = document.getElementById('batches-count-badge');
    const limitFilter = document.getElementById('batch-limit-filter')?.value || '50';
    if (!container) return;

    if (badgeEl) badgeEl.innerText = state.minibatches.length;

    const filtered = getFilteredBatches();
    const limit = limitFilter === 'all' ? filtered.length : parseInt(limitFilter, 10);
    const visibleBatches = filtered.slice(0, limit);

    if (visibleCountEl) {
        visibleCountEl.innerText = `${visibleBatches.length}${filtered.length > visibleBatches.length ? ` of ${filtered.length}` : ''}`;
    }

    if (visibleBatches.length === 0) {
        container.innerHTML = `
            <div class="col-span-full p-8 text-center bg-slate-900/60 rounded-xl border border-slate-800 text-slate-400 text-xs">
                No batches match the selected filters or search query.
            </div>
        `;
        return;
    }

    // Set correct container classes depending on view mode
    if (state.batchViewMode === 'compact') {
        container.className = 'space-y-1.5';
        container.innerHTML = visibleBatches.map(b => renderCompactBatchRow(b)).join('');
    } else {
        container.className = 'grid grid-cols-1 lg:grid-cols-2 gap-4';
        container.innerHTML = visibleBatches.map(b => renderGridBatchCard(b)).join('');
    }
}

// High-Density Compact Batch Row (Perfect for scanning 50+ batches on 1-2 screen heights)
function renderCompactBatchRow(b) {
    const catBadge = getCategoryBadge(b.category);
    const tierBadge = getComplexityBadge(b.complexity, b.prNumbers.length);
    const selectedCount = b.selectedPRs.size;
    const totalPRs = b.prs.length || b.prNumbers.length;
    const isExpanded = state.expandedBatches.has(b.id);

    const isClaimedByMe = b.claimedBy === state.currentMaintainer;
    const isClaimedByOther = b.claimedBy && b.claimedBy !== 'none' && !isClaimedByMe;

    let totalAdds = 0;
    let totalDels = 0;
    const uniqueFiles = new Set();
    b.prs.forEach(p => {
        totalAdds += (p.additions || 0);
        totalDels += (p.deletions || 0);
        (p.files || []).forEach(f => {
            const path = typeof f === 'string' ? f : (f.path || '');
            if (path) uniqueFiles.add(path);
        });
    });

    const statusPill = isClaimedByOther ? `
        <span class="text-[10px] px-2 py-0.5 rounded-full bg-slate-900 text-amber-300 border border-amber-800/80 font-mono whitespace-nowrap flex items-center gap-1">
            <span>🔒</span> @${escapeHtml(b.claimedBy)}
        </span>
    ` : isClaimedByMe ? `
        <span class="text-[10px] px-2 py-0.5 rounded-full bg-cyan-950 text-cyan-300 border border-cyan-800 font-mono whitespace-nowrap flex items-center gap-1">
            <span>👤</span> Yours
        </span>
    ` : `
        <span class="text-[10px] px-2 py-0.5 rounded-full bg-emerald-950 text-emerald-300 border border-emerald-800 font-mono whitespace-nowrap flex items-center gap-1">
            <span>🟢</span> Available
        </span>
    `;

    const actionButton = isCurrentUserMaintainer() ? (
        isClaimedByOther ? `
            <button disabled class="px-2.5 py-1 bg-slate-900 text-slate-500 border border-slate-800 rounded-md text-[11px] font-semibold cursor-not-allowed">
                🔒 Claimed
            </button>
        ` : isClaimedByMe ? `
            <button onclick="releaseBatch('${b.id}')" 
                    class="px-2.5 py-1 bg-rose-950 hover:bg-rose-900 text-rose-300 border border-rose-800 rounded-md text-[11px] font-semibold transition cursor-pointer">
                Release
            </button>
        ` : `
            <button onclick="claimBatchServerSide('${b.id}')" 
                    class="px-2.5 py-1 bg-cyan-600 hover:bg-cyan-500 active:scale-95 text-white font-bold rounded-md text-[11px] shadow-sm shadow-cyan-600/20 transition flex items-center gap-1 cursor-pointer">
                <span>➕</span> Claim Batch (${totalPRs})
            </button>
        `
    ) : `
        <button onclick="addBatchToReviewWatchlist('${b.id}')" 
                class="px-2.5 py-1 bg-purple-950 hover:bg-purple-900 text-purple-300 border border-purple-800 rounded-md text-[11px] font-semibold transition flex items-center gap-1 cursor-pointer">
            <span>🔍</span> Test Batch (${totalPRs})
        </button>
    `;

    return `
        <div class="rounded-xl bg-cardbg border ${isClaimedByOther ? 'border-slate-800/60 opacity-80' : isClaimedByMe ? 'border-cyan-500/50 ring-1 ring-cyan-500/30' : 'border-slate-800 hover:border-slate-700'} transition shadow-sm p-2 sm:p-2.5" id="batch-row-${b.id}">
            <!-- Main Dense Horizontal Bar -->
            <div class="flex items-center justify-between gap-2 flex-wrap lg:flex-nowrap">
                <!-- Left: Expander, Category, Title, Complexity -->
                <div class="flex items-center gap-2 min-w-0 flex-1">
                    <button onclick="toggleBatchExpand('${b.id}')" class="p-1 rounded text-slate-400 hover:text-cyan-300 hover:bg-slate-800 transition text-[11px] flex-shrink-0 cursor-pointer" title="Show/hide individual PRs">
                        ${isExpanded ? '▼' : '▶'}
                    </button>
                    ${catBadge}
                    <span class="font-bold text-xs text-slate-100 hover:text-cyan-300 cursor-pointer truncate" onclick="toggleBatchExpand('${b.id}')" title="${escapeHtml(b.title)}">
                        ${escapeHtml(b.title)}
                    </span>
                    ${tierBadge}
                </div>

                <!-- Middle: Aggregate Diff, Disjoint Files, Clickable PR Pills -->
                <div class="flex items-center gap-2 text-xs font-mono flex-shrink-0">
                    <div class="text-[11px] whitespace-nowrap">
                        <span class="text-emerald-400 font-bold">+${totalAdds}</span>/<span class="text-rose-400 font-bold">-${totalDels}</span>
                    </div>
                    <span class="text-slate-400 text-[10px] hidden md:inline px-1.5 py-0.5 rounded bg-slate-900 border border-slate-800 whitespace-nowrap" title="Guaranteed disjoint files without mutual conflicts">
                        📁 ${uniqueFiles.size} files
                    </span>

                    <!-- Clickable PR pills with hover titles -->
                    <div class="flex items-center gap-1 flex-wrap">
                        ${b.prs.slice(0, 6).map(pr => `
                            <span onclick="openInspectModal(${pr.number})" title="${escapeHtml(pr.title)} (${pr.author})" class="px-1.5 py-0.5 rounded bg-slate-900/90 hover:bg-cyan-950 text-cyan-400 border border-slate-800 hover:border-cyan-700 text-[10px] font-bold cursor-pointer transition whitespace-nowrap">
                                #${pr.number}
                            </span>
                        `).join('')}
                        ${b.prs.length > 6 ? `<span class="text-[10px] text-slate-500 font-mono cursor-pointer" onclick="toggleBatchExpand('${b.id}')">+${b.prs.length - 6} more</span>` : ''}
                    </div>
                </div>

                <!-- Right: Status + 1-Click AI Prompt + CLI + Claim -->
                <div class="flex items-center gap-1.5 flex-shrink-0 ml-auto lg:ml-0">
                    ${statusPill}
                    
                    <!-- 1-Click Deterministic AI Prompt Copy Button -->
                    <button onclick="copyBatchAIPrompt('${b.id}', event)" class="px-2 py-1 bg-indigo-950 hover:bg-indigo-900 active:scale-95 text-indigo-300 border border-indigo-800 rounded-md text-[11px] font-semibold transition flex items-center gap-1 cursor-pointer" title="Copy deterministic prompt for Claude / GPT-4o / Gemini">
                        <span>🤖</span> <span class="hidden sm:inline">AI Prompt</span>
                    </button>

                    <!-- CLI Script Modal Button -->
                    <button onclick="openBatchScriptModal('${b.id}', 'merge')" class="px-2 py-1 bg-slate-800 hover:bg-slate-700 text-slate-300 border border-slate-700 rounded-md text-[11px] font-mono transition flex items-center gap-1 cursor-pointer" title="Show CLI Merge / Test scripts">
                        <span>⚡</span> <span class="hidden sm:inline">CLI</span>
                    </button>

                    ${actionButton}
                </div>
            </div>

            <!-- Expandable Detailed PR List (Batch-Only Invariant) -->
            ${isExpanded ? `
                <div class="mt-2.5 pt-2 border-t border-slate-800/80 space-y-1">
                    <div class="flex items-center justify-between text-[11px] text-slate-400 px-1 pb-1">
                        <span>Pull Requests in this Batch: <strong class="text-cyan-400">${totalPRs} PRs</strong> (all PRs touch disjoint files)</span>
                        <button onclick="openBatchScriptModal('${b.id}', 'ai-prompt')" class="text-cyan-400 hover:underline flex items-center gap-1 cursor-pointer">
                            <span>🤖</span> View full AI Prompt in modal &rarr;
                        </button>
                    </div>
                    <div class="space-y-1 max-h-60 overflow-y-auto pr-1">
                        ${b.prs.map(pr => `
                            <div class="flex items-center justify-between p-1.5 rounded-lg bg-slate-900/80 border border-slate-800/80 text-xs hover:border-slate-700 transition">
                                <div class="flex items-center gap-2 flex-1 min-w-0 pr-2">
                                    <span class="w-1.5 h-1.5 rounded-full bg-cyan-400 flex-shrink-0"></span>
                                    <span class="font-mono font-bold text-cyan-400 text-[11px]">#${pr.number}</span>
                                    <span class="text-[11px] text-slate-200 truncate cursor-pointer hover:text-cyan-300" onclick="openInspectModal(${pr.number})" title="${escapeHtml(pr.title)}">
                                        ${escapeHtml(pr.title)}
                                    </span>
                                </div>
                                <div class="flex items-center gap-2 flex-shrink-0 text-[10px] font-mono">
                                    <span class="text-slate-400 text-[10px]">@${escapeHtml(pr.author)}</span>
                                    <span class="text-emerald-400">+${pr.additions}</span>/<span class="text-rose-400">-${pr.deletions}</span>
                                    <button onclick="openInspectModal(${pr.number})" class="text-cyan-400 hover:text-cyan-300 p-0.5 cursor-pointer">🔍</button>
                                </div>
                            </div>
                        `).join('')}
                    </div>
                </div>
            ` : ''}
        </div>
    `;
}

// Grid Card View (Alternative mode for Minibatches)
function renderGridBatchCard(b) {
    const catBadge = getCategoryBadge(b.category);
    const tierBadge = getComplexityBadge(b.complexity, b.prNumbers.length);
    const totalPRs = b.prs.length || b.prNumbers.length;

    const isClaimedByMe = b.claimedBy === state.currentMaintainer;
    const isClaimedByOther = b.claimedBy && b.claimedBy !== 'none' && !isClaimedByMe;

    return `
        <div class="p-4 rounded-xl bg-cardbg border ${isClaimedByOther ? 'border-slate-800/60 opacity-80' : isClaimedByMe ? 'border-cyan-500/50 ring-1 ring-cyan-500/30' : 'border-slate-800 hover:border-slate-700'} transition shadow-sm space-y-3 flex flex-col justify-between" id="card-${b.id}">
            <div class="space-y-2">
                <!-- Batch Card Header -->
                <div class="flex items-center justify-between gap-2 flex-wrap">
                    <div class="flex items-center gap-1.5 flex-wrap">
                        ${catBadge}
                        ${tierBadge}
                    </div>
                    ${isClaimedByOther ? `
                        <span class="text-[10px] px-2 py-0.5 rounded-full bg-slate-900 text-amber-300 border border-amber-800/80 font-mono flex items-center gap-1">
                            <span>🔒</span> Claimed by @${escapeHtml(b.claimedBy)}
                        </span>
                    ` : isClaimedByMe ? `
                        <span class="text-[10px] px-2 py-0.5 rounded-full bg-cyan-950 text-cyan-300 border border-cyan-800 font-mono flex items-center gap-1">
                            <span>👤</span> Your claimed batch
                        </span>
                    ` : `
                        <span class="text-[10px] px-2 py-0.5 rounded-full bg-emerald-950 text-emerald-300 border border-emerald-800 font-mono flex items-center gap-1">
                            <span>🟢</span> Available on Supabase
                        </span>
                    `}
                </div>

                <div>
                    <h3 class="text-sm font-bold text-slate-100 flex items-center justify-between">
                        <span>${escapeHtml(b.title)}</span>
                        <span class="text-xs font-mono text-cyan-400">${totalPRs} PRs</span>
                    </h3>
                    <p class="text-[11px] text-slate-400 mt-0.5">
                        Batch contains ${totalPRs} PRs with disjoint files. 100% conflict-free merge.
                    </p>
                </div>

                <!-- PR Items inside batch (Batch-Only Invariant) -->
                <div class="space-y-1.5 pt-1 max-h-56 overflow-y-auto pr-1">
                    ${b.prs.map(pr => `
                        <div class="flex items-center justify-between p-2 rounded-lg bg-slate-900/90 border border-slate-800/80 text-xs hover:border-slate-700 transition">
                            <div class="flex items-center gap-2 flex-1 min-w-0 pr-2">
                                <span class="w-1.5 h-1.5 rounded-full bg-cyan-400 flex-shrink-0"></span>
                                <span class="font-mono font-bold text-cyan-400 text-[11px]">#${pr.number}</span>
                                <span class="text-[11px] text-slate-200 truncate cursor-pointer hover:text-cyan-300" onclick="openInspectModal(${pr.number})" title="${escapeHtml(pr.title)}">
                                    ${escapeHtml(pr.title)}
                                </span>
                            </div>
                            <div class="flex items-center gap-1.5 flex-shrink-0 text-[10px] font-mono">
                                <span class="text-emerald-400">+${pr.additions}</span>/<span class="text-rose-400">-${pr.deletions}</span>
                            </div>
                        </div>
                    `).join('')}
                </div>
            </div>

            <!-- Batch Actions (Role-Aware + AI Prompt Button) -->
            <div class="pt-3 border-t border-slate-800 flex items-center justify-between flex-wrap gap-2 text-xs">
                <div class="flex items-center gap-1.5">
                    ${isCurrentUserMaintainer() ? (
                        isClaimedByOther ? `
                            <button disabled class="px-3 py-1.5 bg-slate-900 text-slate-500 border border-slate-800 rounded-lg text-xs font-semibold cursor-not-allowed">
                                🔒 Already Claimed
                            </button>
                        ` : isClaimedByMe ? `
                            <button onclick="releaseBatch('${b.id}')" 
                                     class="px-3 py-1.5 bg-rose-950 hover:bg-rose-900 text-rose-300 border border-rose-800 rounded-lg text-xs font-semibold transition cursor-pointer">
                                Release batch
                            </button>
                        ` : `
                            <button onclick="claimBatchServerSide('${b.id}')" 
                                    class="px-3 py-1.5 bg-cyan-600 hover:bg-cyan-500 active:scale-95 text-white font-bold rounded-lg text-xs shadow-md shadow-cyan-600/20 transition flex items-center gap-1 cursor-pointer">
                                <span>➕</span> Claim Batch (${totalPRs})
                            </button>
                        `
                    ) : `
                        <button onclick="addBatchToReviewWatchlist('${b.id}')" 
                                class="px-3 py-1.5 bg-purple-950 hover:bg-purple-900 text-purple-300 border border-purple-800 rounded-lg text-xs font-semibold transition flex items-center gap-1 cursor-pointer">
                            <span>🔍</span> Test batch (${totalPRs})
                        </button>
                    `}
                </div>

                <div class="flex items-center gap-1.5">
                    <!-- 1-Click AI Prompt Copy -->
                    <button onclick="copyBatchAIPrompt('${b.id}', event)" 
                            class="px-2.5 py-1.5 bg-indigo-950 hover:bg-indigo-900 text-indigo-300 border border-indigo-800 rounded-lg text-xs font-semibold transition flex items-center gap-1 cursor-pointer" title="Copy deterministic prompt">
                        <span>🤖</span> AI Prompt
                    </button>
                    <!-- CLI modal -->
                    <button onclick="openBatchScriptModal('${b.id}', 'merge')" 
                            class="px-2.5 py-1.5 bg-slate-800 hover:bg-slate-700 text-slate-300 border border-slate-700 rounded-lg text-xs font-mono transition flex items-center gap-1 cursor-pointer">
                        <span>⚡</span> CLI
                    </button>
                </div>
            </div>
        </div>
    `;
}

// Deterministic AI Prompt generator for LLMs (Claude, GPT, Gemini)
function generateBatchAIPrompt(batchId) {
    const batch = state.minibatches.find(b => b.id === batchId);
    if (!batch) return '';

    const selectedNums = Array.from(batch.selectedPRs || batch.prNumbers || []);
    const prs = selectedNums.map(n => state.allPRs.find(p => p.number === n)).filter(Boolean);

    let totalAdds = 0;
    let totalDels = 0;
    const allFiles = new Set();

    prs.forEach(p => {
        totalAdds += (p.additions || 0);
        totalDels += (p.deletions || 0);
        const fList = p.files || [];
        fList.forEach(f => {
            if (typeof f === 'string') allFiles.add(f);
            else if (f && f.path) allFiles.add(f.path);
        });
    });
    allFiles.delete('');

    let p = `# AnyPS5 Pull Request Batch Review: ${batch.title}\n\n`;
    p += `## 🎯 OBJECTIVE & ROLE\n`;
    p += `You are an elite principal systems software engineer, C++ architect, and PlayStation 5 emulator core contributor.\n`;
    p += `Your task is to conduct a rigorous, comprehensive architectural and code safety review of a **conflict-free batch of pull requests** destined for the \`boykopovar/AnyPS5\` repository.\n\n`;

    p += `## 📦 BATCH SPECIFICATION\n`;
    p += `- **Batch Identifier**: \`${batch.id}\`\n`;
    p += `- **Subsystem / Domain**: \`${batch.category}\`\n`;
    p += `- **Complexity Tier**: \`${batch.complexityLabel || batch.complexity}\`\n`;
    p += `- **Total Pull Requests**: ${prs.length}\n`;
    p += `- **Disjoint Files Guarantee**: ✅ **CONFIRMED 100% DISJOINT** (PRs inside this batch do NOT share modified files, guaranteeing zero direct git conflicts)\n`;
    p += `- **Aggregate Code Churn**: +${totalAdds} / -${totalDels} lines across ${allFiles.size} unique files\n\n`;
    p += `---\n\n`;

    p += `## 🔍 DETAILED INCLUDED PULL REQUESTS\n\n`;
    prs.forEach((pr, idx) => {
        const rawAuthor = pr.author || 'unknown';
        const author = typeof rawAuthor === 'object' ? (rawAuthor.login || rawAuthor.name || 'unknown') : rawAuthor;
        const prFiles = (pr.files || []).map(f => typeof f === 'string' ? f : (f.path || '')).filter(Boolean);
        const risk = (pr.riskLevel || 'low').toUpperCase();

        p += `### ${idx + 1}. PR #${pr.number}: ${pr.title}\n`;
        p += `- **Author**: @${author}\n`;
        p += `- **Direct Link**: https://github.com/boykopovar/AnyPS5/pull/${pr.number}\n`;
        p += `- **Assessed Risk Level**: \`${risk}\`${pr.riskReason ? ` (${pr.riskReason})` : ''}\n`;
        p += `- **Diff Churn**: +${pr.additions || 0} / -${pr.deletions || 0} lines (${prFiles.length} files)\n`;
        p += `- **Modified Files**:\n`;
        if (prFiles.length > 0) {
            prFiles.forEach(pf => { p += `  - \`${pf}\`\n`; });
        } else {
            p += `  - *(Core internal module files)*\n`;
        }
        p += `\n`;
    });

    p += `---\n\n`;
    p += `## 🛡️ STRICT EMULATION REVIEW DIRECTIVES & INVARIANTS\n\n`;
    p += `Evaluate this batch against critical PS5 emulation architecture principles:\n\n`;
    p += `1. **Concurrency & Thread Safety**:\n`;
    p += `   - Check for data races, mutex deadlocks, lock inversion, or incorrect atomic memory orderings (\`std::memory_order_relaxed\` vs \`acquire/release\`).\n`;
    p += `   - AnyPS5 runs high-throughput multithreaded guest jobs (FreeBSD kernel worker threads, AGC command buffers). Verify all shared state is protected.\n\n`;
    p += `2. **FreeBSD & PS5 Kernel ABI Fidelity**:\n`;
    p += `   - Verify that system calls, ioctls, and POSIX shims strictly match FreeBSD 12/13 ABI semantics and return appropriate negative errno codes where expected.\n`;
    p += `   - Guard against structure padding, endianness assumptions, and 64-bit pointer truncation.\n\n`;
    p += `3. **Graphics (AGC) & Shader Translation Invariants** (if applicable to ${batch.category}):\n`;
    p += `   - Ensure descriptor indexing bounds checks, uniform buffer alignments, and Vulkan/GCN pipeline barriers are sound.\n`;
    p += `   - Verify shader disk cache serialization does not introduce disk corruption or stale key collisions.\n\n`;
    p += `4. **Cross-PR Semantic Side-Effects**:\n`;
    p += `   - While the files are strictly physically disjoint, verify whether any PR introduces assumptions, global state changes, or build flag dependencies that could impact neighboring modules.\n\n`;
    p += `5. **Resource Management**:\n`;
    p += `   - Verify zero memory leaks (prefer RAII, \`std::unique_ptr\`, \`std::shared_ptr\`).\n`;
    p += `   - Validate bounds checking on guest pointer offsets and packet buffers.\n\n`;

    p += `---\n\n`;
    p += `## 📋 REQUIRED OUTPUT FORMAT\n\n`;
    p += `Please organize your response strictly into the following sections:\n\n`;
    p += `### 1. Executive Batch Summary\n`;
    p += `A concise 2-3 paragraph synthesis of the batch's architectural impact, overall stability, and risk level.\n\n`;
    p += `### 2. PR-by-PR Evaluation Table\n`;
    p += `| PR # | Title | Risk | Verdict (Approve / Request Changes / Needs Benchmarking) | Key Notes |\n`;
    p += `|:---:|:---|:---:|:---:|:---|n`;
    prs.forEach(pr => {
        const cleanTitle = (pr.title || '').replace(/\|/g, '-').slice(0, 55);
        p += `| #${pr.number} | ${cleanTitle} | \`${(pr.riskLevel || 'low').toUpperCase()}\` | **[VERDICT]** | [1-sentence assessment] |\n`;
    });
    p += `\n`;
    p += `### 3. Detailed Findings & Potential Regressions\n`;
    p += `Highlight any suspicious code patterns, missing edge cases, or suggested defensive assertions for any specific PRs.\n\n`;
    p += `### 4. Sequential Merge Recommendation\n`;
    p += `Provide the optimal sequential merge order or confirmation to squash-merge with \`gh pr merge\`:\n`;
    p += `\`\`\`bash\n# Recommended Sequence\n`;
    prs.forEach(pr => {
        p += `gh pr merge ${pr.number} --repo boykopovar/AnyPS5 --auto --squash --delete-branch\n`;
    });
    p += `\`\`\`\n`;

    return p;
}

function copyBatchAIPrompt(batchId, event) {
    if (event) event.stopPropagation();
    const batch = state.minibatches.find(b => b.id === batchId);
    if (!batch) return;

    const prompt = generateBatchAIPrompt(batchId);
    copyTextToClipboard(prompt, `🤖 AI Prompt for "${batch.title}" copied! Paste into Claude / GPT / Gemini.`);
}

function togglePRInBatch(batchId, prNumber, checked) {
    const batch = state.minibatches.find(b => b.id === batchId);
    if (!batch) return;

    if (checked) batch.selectedPRs.add(prNumber);
    else batch.selectedPRs.delete(prNumber);

    renderMinibatches();
}

// Atomic Server-Side Batch Claiming (Batch-Only Invariant: PRs claimed as atomic unit)
async function claimBatchServerSide(batchId) {
    const batch = state.minibatches.find(b => b.id === batchId);
    if (!batch) return;

    const allBatchPRs = batch.prNumbers || [];
    if (allBatchPRs.length === 0) {
        showToast('Batch contains no PRs');
        return;
    }

    try {
        // 1. Atomically claim batch in public.batches table
        const res = await fetch(`${SUPABASE_URL}/rest/v1/batches?id=eq.${batchId}&status=eq.available`, {
            method: 'PATCH',
            headers: {
                ...supabaseHeaders,
                'Prefer': 'return=representation'
            },
            body: JSON.stringify({
                status: 'claimed',
                claimed_by: state.currentMaintainer,
                claimed_at: new Date().toISOString(),
                updated_at: new Date().toISOString()
            })
        });

        const updatedData = await res.json();
        if (!Array.isArray(updatedData) || updatedData.length === 0) {
            showToast('⚠️ This batch was just claimed by another maintainer!');
            refreshDataFromSupabase();
            return;
        }

        // 2. Assign all PRs in public.prs table
        await updateBatchPRsInSupabase(allBatchPRs, {
            assigned_reviewer: state.currentMaintainer,
            status: 'in-review'
        });

        batch.status = 'claimed';
        batch.claimedBy = state.currentMaintainer;
        batch.claimedAt = new Date().toISOString();

        allBatchPRs.forEach(num => {
            const pr = state.allPRs.find(p => p.number === num);
            if (pr) {
                pr.assignedReviewer = state.currentMaintainer;
                pr.status = 'in-review';
            }
        });

        renderMyPRs();
        renderMinibatches();
        renderRepoTable();

        showToast(`Successfully claimed and locked batch "${batch.title}" (${allBatchPRs.length} PRs)!`);
    } catch (e) {
        console.error('Batch claim error:', e);
        showToast('Error claiming batch from server');
    }
}

// Release a batch back to available state on Supabase
async function releaseBatch(batchId) {
    const batch = state.minibatches.find(b => b.id === batchId);
    if (!batch) return;

    try {
        await fetch(`${SUPABASE_URL}/rest/v1/batches?id=eq.${batchId}`, {
            method: 'PATCH',
            headers: {
                ...supabaseHeaders,
                'Prefer': 'return=minimal'
            },
            body: JSON.stringify({
                status: 'available',
                claimed_by: 'none',
                claimed_at: null,
                updated_at: new Date().toISOString()
            })
        });

        await updateBatchPRsInSupabase(batch.prNumbers, {
            assigned_reviewer: 'none',
            status: 'unassigned'
        });

        batch.status = 'available';
        batch.claimedBy = 'none';
        batch.claimedAt = null;

        batch.prNumbers.forEach(num => {
            const pr = state.allPRs.find(p => p.number === num);
            if (pr) {
                pr.assignedReviewer = 'none';
                pr.status = 'unassigned';
            }
        });

        renderMyPRs();
        renderMinibatches();
        renderRepoTable();

        showToast(`Batch "${batch.title}" released and returned to Supabase!`);
    } catch (e) {
        console.error('Release batch error:', e);
    }
}

function regenerateMinibatches() {
    refreshDataFromSupabase(true);
}

function getComplexityBadge(tier, count) {
    if (tier === 'low') {
        return `<span class="px-2 py-0.5 rounded text-[10px] font-bold bg-emerald-950 text-emerald-300 border border-emerald-800">🟢 Easy (${count} PRs)</span>`;
    } else if (tier === 'medium') {
        return `<span class="px-2 py-0.5 rounded text-[10px] font-semibold bg-amber-950 text-amber-300 border border-amber-800">🟡 Medium (${count} PRs)</span>`;
    } else {
        return `<span class="px-2 py-0.5 rounded text-[10px] font-bold bg-rose-950 text-rose-300 border border-rose-800">🔴 Complex (${count} PRs)</span>`;
    }
}

// Add batch PRs to Community Reviewer Watchlist
function addBatchToReviewWatchlist(batchId) {
    const batch = state.minibatches.find(b => b.id === batchId);
    if (!batch) return;

    batch.prNumbers.forEach(num => state.reviewerWatchlist.add(num));
    savePersistentUser();
    renderMyPRs();
    renderMinibatches();
    showToast(`Added ${batch.prNumbers.length} PRs from "${batch.title}" to your review workbench!`);
}

// Modal for Batch Tools (AI Review Prompt, CLI Merge, and CLI Test)
function openBatchScriptModal(batchId, initialTab = 'ai-prompt') {
    const batch = state.minibatches.find(b => b.id === batchId);
    if (!batch) return;

    state.activeBatchForModal = batch;
    state.activeBatchModalTab = initialTab;

    const modal = document.getElementById('batch-script-modal');
    const title = document.getElementById('batch-script-title');
    if (!modal || !title) return;

    title.innerText = `${batch.title} (${batch.prNumbers.length} PRs)`;
    switchBatchModalTab(initialTab);

    modal.classList.remove('hidden');
    modal.classList.add('flex');
}

function switchBatchModalTab(tab) {
    state.activeBatchModalTab = tab;
    const batch = state.activeBatchForModal;
    if (!batch) return;

    const tabAi = document.getElementById('modal-tab-ai');
    const tabMerge = document.getElementById('modal-tab-merge');
    const tabTest = document.getElementById('modal-tab-test');
    const desc = document.getElementById('batch-script-desc');
    const hint = document.getElementById('batch-script-hint');
    const textarea = document.getElementById('batch-script-textarea');
    const btnText = document.getElementById('batch-script-copy-btn-text');

    const inactiveClass = 'px-3 py-1.5 rounded-lg text-xs font-medium transition text-slate-400 hover:text-slate-200 border border-transparent hover:bg-slate-800 flex items-center gap-1.5 whitespace-nowrap cursor-pointer';
    const activeClass = 'px-3 py-1.5 rounded-lg text-xs font-semibold transition bg-cyan-950 text-cyan-300 border border-cyan-800 flex items-center gap-1.5 whitespace-nowrap shadow-sm cursor-pointer';

    if (tabAi) tabAi.className = tab === 'ai-prompt' ? activeClass : inactiveClass;
    if (tabMerge) tabMerge.className = tab === 'merge' ? activeClass : inactiveClass;
    if (tabTest) tabTest.className = tab === 'test' ? activeClass : inactiveClass;

    const selectedNums = Array.from(batch.selectedPRs || batch.prNumbers || []);

    if (tab === 'ai-prompt') {
        if (desc) desc.innerText = 'This deterministic prompt contains the full context, disjoint file lists, PS5 architectural guidelines, and review rubric. Ready to paste into Claude 3.7 Sonnet, OpenAI o3/GPT-4o, or Gemini Pro.';
        if (hint) hint.innerHTML = '100% deterministic prompt (0 API cost) – paste into AI chat for deep batch review.';
        if (btnText) btnText.innerText = 'Copy AI Prompt';
        if (textarea) textarea.value = generateBatchAIPrompt(batch.id);
    } else if (tab === 'merge') {
        if (desc) desc.innerText = 'This script sequentially merges all selected PRs from this batch. Because files are disjoint, merging succeeds 100% conflict-free!';
        if (hint) hint.innerHTML = 'Copy and run in terminal with <code class="text-slate-400 font-mono">gh</code> CLI.';
        if (btnText) btnText.innerText = 'Copy Merge Script';

        let script = `# AnyPS5 Minibatch CLI Merge Sequence\n`;
        script += `# Batch: ${batch.title} (${batch.category})\n`;
        script += `# Complexity: ${batch.complexityLabel || batch.complexity} • Disjoint files: 100% Conflict-free merge\n`;
        script += `# Generated: ${new Date().toISOString()}\n\n`;

        selectedNums.forEach(num => {
            const pr = state.allPRs.find(p => p.number === num);
            const prTitle = pr ? pr.title.replace(/[\r\n]+/g, ' ') : '';
            script += `# PR #${num}: ${prTitle}\n`;
            script += `gh pr merge ${num} --repo boykopovar/AnyPS5 --auto --squash --delete-branch\n\n`;
        });
        if (textarea) textarea.value = script;
    } else if (tab === 'test') {
        if (desc) desc.innerText = 'As a reviewer or maintainer, this script allows you to locally fetch branches and compile all PRs from this batch:';
        if (hint) hint.innerHTML = 'Copy and run in your local AnyPS5 clone to build and run test suites.';
        if (btnText) btnText.innerText = 'Copy Test Script';

        let script = `# AnyPS5 Minibatch Local Test Sequence\n`;
        script += `# Batch: ${batch.title} (${batch.category})\n`;
        script += `# Generated: ${new Date().toISOString()}\n\n`;
        script += `git fetch origin\n`;
        script += `git checkout -b test-${batch.id}\n\n`;

        selectedNums.forEach(num => {
            const pr = state.allPRs.find(p => p.number === num);
            const prTitle = pr ? pr.title.replace(/[\r\n]+/g, ' ') : '';
            script += `# PR #${num}: ${prTitle}\n`;
            script += `gh pr checkout ${num}\n`;
            script += `cmake --build build --config Release\n`;
            script += `ctest --test-dir build --output-on-failure\n\n`;
        });
        if (textarea) textarea.value = script;
    }
}

function closeBatchScriptModal() {
    const modal = document.getElementById('batch-script-modal');
    if (modal) {
        modal.classList.add('hidden');
        modal.classList.remove('flex');
    }
}

function copyBatchScriptText() {
    const textarea = document.getElementById('batch-script-textarea');
    if (textarea) {
        const isAi = state.activeBatchModalTab === 'ai-prompt';
        const msg = isAi 
            ? '🤖 AI Prompt copied to clipboard! Paste into Claude / GPT / Gemini.'
            : '💻 CLI script copied to clipboard!';
        copyTextToClipboard(textarea.value, msg);
        closeBatchScriptModal();
    }
}


/// ==========================================
// VIEW 1: MY CLAIMED BATCHES (BATCH-ONLY WORKFLOW)
// ==========================================

function findBatchForPR(prNumber) {
    return state.minibatches.find(b => (b.prNumbers || []).includes(prNumber));
}

function jumpToBatch(batchId) {
    if (!batchId) return;
    switchMainView('batches');
    const searchInput = document.getElementById('batch-search-input');
    if (searchInput) searchInput.value = batchId;
    const statusFilter = document.getElementById('batch-status-filter');
    if (statusFilter) statusFilter.value = 'all';
    state.expandedBatches.add(batchId);
    renderMinibatches();
    setTimeout(() => {
        const el = document.getElementById(`batch-row-${batchId}`) || document.getElementById(`card-${batchId}`);
        if (el) {
            el.scrollIntoView({ behavior: 'smooth', block: 'center' });
            el.classList.add('ring-2', 'ring-cyan-400');
            setTimeout(() => el.classList.remove('ring-2', 'ring-cyan-400'), 2500);
        }
    }, 100);
}

function jumpToParentBatchFromInspect() {
    if (!state.activeInspectPR) return;
    const batch = findBatchForPR(state.activeInspectPR.number);
    if (batch) {
        closeInspectModal();
        jumpToBatch(batch.id);
    }
}

function renderMyPRs() {
    const container = document.getElementById('my-prs-container');
    const emptyState = document.getElementById('my-prs-empty');
    if (!container) return;

    const isM = isCurrentUserMaintainer();
    
    // In batch-only workflow, View 1 displays claimed batches
    const myBatches = isM 
        ? state.minibatches.filter(b => b.claimedBy === state.currentMaintainer)
        : state.minibatches.filter(b => b.claimedBy === state.currentMaintainer || (b.prNumbers && b.prNumbers.some(num => state.reviewerWatchlist.has(num))));
    
    // Update badge in top bar (Number of claimed batches)
    const badge = document.getElementById('my-pr-count-badge');
    if (badge) badge.innerText = myBatches.length;

    // Update status pills counts
    updateMyBatchCategoryCounts(myBatches);

    // Apply active category filter for batches
    let visibleBatches = myBatches;
    if (state.myStatusFilter && state.myStatusFilter !== 'all') {
        visibleBatches = myBatches.filter(b => b.category === state.myStatusFilter);
    }

    if (myBatches.length === 0) {
        container.innerHTML = '';
        if (emptyState) {
            emptyState.classList.remove('hidden');
        }
        renderSidebarStats();
        return;
    } else {
        if (emptyState) emptyState.classList.add('hidden');
    }

    if (visibleBatches.length === 0) {
        container.innerHTML = `
            <div class="p-8 text-center bg-slate-900/60 rounded-xl border border-slate-800 text-slate-400 text-xs">
                No claimed batches in this category. <button onclick="resetMyStatusFilter()" class="text-cyan-400 underline ml-1 cursor-pointer">Show all</button>
            </div>
        `;
        return;
    }

    container.innerHTML = visibleBatches.map(b => {
        const catBadge = getCategoryBadge(b.category);
        const tierBadge = getComplexityBadge(b.complexity, b.prNumbers.length);
        const totalPRs = b.prs.length || b.prNumbers.length;
        const isExpanded = state.expandedBatches.has(b.id);

        let totalAdds = 0;
        let totalDels = 0;
        const uniqueFiles = new Set();
        b.prs.forEach(p => {
            totalAdds += (p.additions || 0);
            totalDels += (p.deletions || 0);
            (p.files || []).forEach(f => {
                const path = typeof f === 'string' ? f : (f.path || '');
                if (path) uniqueFiles.add(path);
            });
        });

        return `
            <div class="p-4 sm:p-5 rounded-2xl bg-cardbg/95 border border-cyan-500/40 ring-1 ring-cyan-500/20 shadow-xl space-y-3.5 transition" id="my-batch-${b.id}">
                <!-- Header -->
                <div class="flex items-center justify-between gap-2 flex-wrap">
                    <div class="flex items-center gap-2 flex-wrap">
                        <span class="font-mono text-cyan-400 font-black text-sm bg-cyan-950/80 px-2.5 py-0.5 rounded border border-cyan-800">
                            ${b.id.toUpperCase()}
                        </span>
                        ${catBadge}
                        ${tierBadge}
                        <span class="text-[10px] px-2 py-0.5 rounded-full bg-emerald-950 text-emerald-300 border border-emerald-800 font-mono">
                            📁 ${uniqueFiles.size} disjoint files (0 collisions)
                        </span>
                    </div>
                    <div class="text-xs font-mono">
                        <span class="text-emerald-400 font-bold">+${totalAdds}</span> / <span class="text-rose-400 font-bold">-${totalDels}</span> lines
                    </div>
                </div>

                <!-- Title & Batch Meta -->
                <div>
                    <h3 class="text-base font-bold text-white flex items-center justify-between">
                        <span>${escapeHtml(b.title)}</span>
                        <span class="text-xs font-mono text-cyan-300">${totalPRs} PRs</span>
                    </h3>
                    <p class="text-xs text-slate-300 mt-1">
                        Atomic conflict-free batch claimed and locked to your workspace. All member PRs touch separate files.
                    </p>
                </div>

                <!-- Batch Action Tools (AI Prompt, CLI Merge, CLI Test, Release) -->
                <div class="p-2.5 rounded-xl bg-slate-900/90 border border-slate-800 flex items-center justify-between flex-wrap gap-2 text-xs">
                    <div class="flex items-center gap-1.5 flex-wrap">
                        <button onclick="copyBatchAIPrompt('${b.id}', event)" class="px-3 py-1.5 bg-indigo-950 hover:bg-indigo-900 active:scale-95 text-indigo-300 border border-indigo-800 rounded-lg text-xs font-semibold transition flex items-center gap-1.5 cursor-pointer" title="Copy deterministic prompt for Claude / GPT / Gemini">
                            <span>🤖</span> <span>Copy AI Prompt</span>
                        </button>
                        <button onclick="openBatchScriptModal('${b.id}', 'merge')" class="px-3 py-1.5 bg-slate-800 hover:bg-slate-700 text-slate-200 border border-slate-700 rounded-lg text-xs font-mono transition flex items-center gap-1.5 cursor-pointer" title="gh CLI batch merge script">
                            <span>💻</span> <span>CLI Merge</span>
                        </button>
                        <button onclick="openBatchScriptModal('${b.id}', 'test')" class="px-3 py-1.5 bg-slate-800 hover:bg-slate-700 text-slate-200 border border-slate-700 rounded-lg text-xs font-mono transition flex items-center gap-1.5 cursor-pointer" title="Local branch checkout & test script">
                            <span>🔍</span> <span>CLI Test</span>
                        </button>
                    </div>

                    <div class="flex items-center gap-1.5">
                        <button onclick="releaseBatch('${b.id}')" class="px-3 py-1.5 bg-rose-950 hover:bg-rose-900 text-rose-300 border border-rose-800 rounded-lg text-xs font-semibold transition cursor-pointer flex items-center gap-1">
                            <span>🔓</span> Release Batch
                        </button>
                    </div>
                </div>

                <!-- Detailed PRs inside Batch -->
                <div class="space-y-1.5 pt-1">
                    <div class="flex items-center justify-between text-xs text-slate-400 px-1">
                        <span class="font-semibold text-slate-300">Pull Requests in this Batch (${totalPRs}):</span>
                        <button onclick="toggleBatchExpand('${b.id}')" class="text-cyan-400 hover:underline text-[11px] cursor-pointer">
                            ${isExpanded ? '▲ Hide PR Details' : '▼ Show All ' + totalPRs + ' PRs'}
                        </button>
                    </div>

                    <div class="space-y-1.5 ${isExpanded ? '' : 'max-h-56 overflow-y-auto pr-1'}">
                        ${b.prs.map(pr => `
                            <div class="flex items-center justify-between p-2 sm:p-2.5 rounded-xl bg-slate-900/80 border border-slate-800/80 hover:border-slate-700 text-xs transition">
                                <div class="flex items-center gap-2 flex-1 min-w-0 pr-2">
                                    <span class="w-1.5 h-1.5 rounded-full bg-cyan-400 flex-shrink-0"></span>
                                    <span class="font-mono font-bold text-cyan-400 text-xs">#${pr.number}</span>
                                    <span class="text-xs text-slate-200 truncate cursor-pointer hover:text-cyan-300 font-medium" onclick="openInspectModal(${pr.number})" title="${escapeHtml(pr.title)}">
                                        ${escapeHtml(pr.title)}
                                    </span>
                                </div>
                                <div class="flex items-center gap-2 flex-shrink-0 text-[11px] font-mono">
                                    <span class="text-slate-400 hidden sm:inline">@${escapeHtml(pr.author)}</span>
                                    <span class="text-emerald-400">+${pr.additions}</span>/<span class="text-rose-400">-${pr.deletions}</span>
                                    ${isM ? `
                                        <select onchange="quickUpdateMyStatus(${pr.number}, this.value)" class="bg-slate-950 border border-slate-700 text-slate-300 text-[11px] rounded px-1.5 py-0.5 focus:outline-none cursor-pointer">
                                            <option value="in-review" ${pr.status === 'in-review' || pr.status === 'unassigned' ? 'selected' : ''}>🔍 In Review</option>
                                            <option value="approved" ${pr.status === 'approved' ? 'selected' : ''}>✅ Approved</option>
                                            <option value="changes" ${pr.status === 'changes' ? 'selected' : ''}>⚠️ Changes</option>
                                            <option value="ready" ${pr.status === 'ready' ? 'selected' : ''}>🚀 Ready</option>
                                        </select>
                                    ` : ''}
                                    <button onclick="openInspectModal(${pr.number})" class="px-2 py-1 bg-cyan-950 hover:bg-cyan-900 text-cyan-300 border border-cyan-800 rounded text-[11px] font-semibold cursor-pointer">
                                        🧪 Inspect
                                    </button>
                                </div>
                            </div>
                        `).join('')}
                    </div>
                </div>
            </div>
        `;
    }).join('');

    renderSidebarStats();
}

function updateMyBatchCategoryCounts(myBatches) {
    const elAll = document.getElementById('my-stat-all');
    const elAgc = document.getElementById('my-stat-agc');
    const elPrx = document.getElementById('my-stat-prx');
    const elKernel = document.getElementById('my-stat-kernel');

    if (elAll) elAll.innerText = myBatches.length;
    if (elAgc) elAgc.innerText = myBatches.filter(b => b.category === 'AGC & Graphics').length;
    if (elPrx) elPrx.innerText = myBatches.filter(b => b.category === 'PRX High-Level Modules').length;
    if (elKernel) elKernel.innerText = myBatches.filter(b => b.category === 'Kernel & Syscalls').length;
}

function filterMyBatches(cat, element) {
    state.myStatusFilter = cat;
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
    filterMyBatches('all', firstPill);
}

async function quickUpdateMyStatus(prNumber, newStatus) {
    const target = state.allPRs.find(p => p.number === prNumber);
    if (target) target.status = newStatus;
    renderMyPRs();

    await updatePRInSupabase(prNumber, { status: newStatus });
    showToast(`Status updated: ${newStatus}`);
}

async function assignPR(prNumber, maintainerId, renderNow = true) {
    const targetMaintainer = maintainerId || state.currentMaintainer || 'boykopovar';
    const target = state.allPRs.find(p => p.number === prNumber);
    const newStatus = target && target.status === 'unassigned' ? 'in-review' : (target?.status || 'in-review');
    
    if (target) {
        target.assignedReviewer = targetMaintainer;
        target.status = newStatus;
    }

    if (renderNow) {
        renderMyPRs();
        renderMinibatches();
        renderRepoTable();
        renderSidebarStats();
    }

    await updatePRInSupabase(prNumber, { 
        assigned_reviewer: targetMaintainer, 
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
    renderMinibatches();
    renderRepoTable();
    renderSidebarStats();

    await updatePRInSupabase(prNumber, { 
        assigned_reviewer: 'none', 
        status: 'unassigned' 
    });
    showToast(`PR #${prNumber} returned to repository`);
}


// ==========================================
// MODAL: CLAIM NEW PRs (CLAIM MODAL)
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
                        + Claim
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
    if (msg) msg.innerText = `${state.selectedClaimPRs.size} PRs selected`;
}

function quickClaimSingle(prNumber) {
    assignPR(prNumber, state.currentMaintainer, true);
    renderClaimList();
    showToast(`Claimed PR #${prNumber}`);
}

async function confirmSelectedClaims() {
    if (state.selectedClaimPRs.size === 0) {
        showToast('Select at least one PR checkbox');
        return;
    }
    const count = state.selectedClaimPRs.size;
    const selected = Array.from(state.selectedClaimPRs);
    
    selected.forEach(num => assignPR(num, state.currentMaintainer, false));
    closeClaimModal();
    renderMyPRs();
    renderMinibatches();
    renderRepoTable();

    await updateBatchPRsInSupabase(selected, {
        assigned_reviewer: state.currentMaintainer,
        status: 'in-review'
    });
    showToast(`Added ${count} PRs to your queue!`);
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
        showToast('No available PRs in this category');
        return;
    }

    const numbers = toClaim.map(p => p.number);
    numbers.forEach(num => assignPR(num, state.currentMaintainer, false));
    closeClaimModal();
    renderMyPRs();
    renderMinibatches();
    renderRepoTable();

    await updateBatchPRsInSupabase(numbers, {
        assigned_reviewer: state.currentMaintainer,
        status: 'in-review'
    });
    showToast(`Claimed ${toClaim.length} PRs for @${state.currentMaintainer}!`);
}


// ==========================================
// MODAL: PR INSPECTION DRAWER (INSPECT MODAL)
// ==========================================

function openInspectModal(prNumber) {
    const pr = state.allPRs.find(p => p.number === prNumber);
    if (!pr) return;

    state.activeInspectPR = pr;
    const modal = document.getElementById('inspect-modal');
    if (!modal) return;

    const isM = isCurrentUserMaintainer();

    // Header info
    document.getElementById('inspect-pr-num').innerText = `#${pr.number}`;
    document.getElementById('inspect-pr-title').innerText = pr.title;
    document.getElementById('inspect-pr-author').innerText = `@${pr.author}`;
    document.getElementById('inspect-pr-date').innerText = `${formatDate(pr.createdAt)} (${formatRelativeTime(pr.createdAt)})`;
    document.getElementById('inspect-pr-cat').innerHTML = getCategoryBadge(pr.category);
    document.getElementById('inspect-pr-risk').innerHTML = getRiskBadge(pr.riskLevel);
    document.getElementById('inspect-github-link').href = pr.url;

    // Live GitHub PR Status Badge
    const liveBadge = document.getElementById('inspect-github-live-badge');
    const liveText = document.getElementById('inspect-github-live-text');
    if (liveBadge && liveText) {
        liveBadge.className = 'px-2 py-0.5 rounded text-[10px] font-mono bg-slate-900 border border-slate-700 text-slate-300 flex items-center gap-1';
        liveBadge.innerHTML = '<span class="w-1.5 h-1.5 rounded-full bg-cyan-400 animate-pulse"></span><span id="inspect-github-live-text">GitHub API: checking...</span>';
        
        GitHubClient.fetchLivePR(pr.number).then(liveData => {
            if (!state.activeInspectPR || state.activeInspectPR.number !== pr.number) return;
            const textEl = document.getElementById('inspect-github-live-text');
            if (!liveData) {
                if (textEl) textEl.innerText = `GitHub: #${pr.number}`;
                return;
            }
            if (liveData.merged) {
                liveBadge.className = 'px-2 py-0.5 rounded text-[10px] font-mono bg-purple-950 border border-purple-700 text-purple-300 flex items-center gap-1';
                liveBadge.innerHTML = '🟣 <span>GitHub: Merged</span>';
            } else if (liveData.state === 'closed') {
                liveBadge.className = 'px-2 py-0.5 rounded text-[10px] font-mono bg-rose-950 border border-rose-700 text-rose-300 flex items-center gap-1';
                liveBadge.innerHTML = '🔴 <span>GitHub: Closed</span>';
            } else {
                if (liveData.mergeable === true) {
                    liveBadge.className = 'px-2 py-0.5 rounded text-[10px] font-mono bg-emerald-950 border border-emerald-700 text-emerald-300 flex items-center gap-1';
                    liveBadge.innerHTML = '🟢 <span>GitHub: Open (No conflicts)</span>';
                } else if (liveData.mergeable === false) {
                    liveBadge.className = 'px-2 py-0.5 rounded text-[10px] font-mono bg-amber-950 border border-amber-700 text-amber-300 flex items-center gap-1';
                    liveBadge.innerHTML = '⚠️ <span>GitHub: Branch conflict</span>';
                } else {
                    liveBadge.className = 'px-2 py-0.5 rounded text-[10px] font-mono bg-cyan-950 border border-cyan-700 text-cyan-300 flex items-center gap-1';
                    liveBadge.innerHTML = '🟢 <span>GitHub: Open</span>';
                }
            }
        }).catch(() => {
            const textEl = document.getElementById('inspect-github-live-text');
            if (textEl) textEl.innerText = `GitHub: #${pr.number}`;
        });
    }

    // Role Indicator Chip in Inspect Modal
    const roleChip = document.getElementById('inspect-user-role-chip');
    const tool4Title = document.getElementById('inspect-tool4-title');
    const maintainerVerdictSec = document.getElementById('maintainer-verdict-section');
    const reviewerRecSec = document.getElementById('reviewer-recommendation-section');
    const maintainerFooterBtns = document.getElementById('inspect-footer-maintainer-btns');
    const reviewerFooterBtns = document.getElementById('inspect-footer-reviewer-btns');
    const submitReviewBtn = document.getElementById('inspect-submit-review-btn');

    if (roleChip) {
        roleChip.className = isM 
            ? 'text-[10px] px-2 py-0.5 rounded-full font-bold bg-emerald-950 text-emerald-300 border border-emerald-800'
            : 'text-[10px] px-2 py-0.5 rounded-full font-bold bg-purple-950 text-purple-300 border border-purple-800';
        roleChip.innerText = isM ? '🛡️ Maintainer Mode' : '🔍 Reviewer Mode';
    }

    if (tool4Title) {
        tool4Title.innerText = isM ? 'Tool 4: Official Maintainer Verdict & Notes' : 'Tool 4: Reviewer Recommendation & Markdown Export';
    }

    if (maintainerVerdictSec) maintainerVerdictSec.classList.toggle('hidden', !isM);
    if (reviewerRecSec) reviewerRecSec.classList.toggle('hidden', isM);
    if (maintainerFooterBtns) maintainerFooterBtns.classList.toggle('hidden', !isM);
    if (reviewerFooterBtns) reviewerFooterBtns.classList.toggle('hidden', isM);

    if (submitReviewBtn) {
        submitReviewBtn.href = `${pr.url}/files`;
    }

    // Parent Batch Information (Batch-Only Invariant)
    const parentBatch = findBatchForPR(pr.number);
    const parentBatchPill = document.getElementById('inspect-parent-batch-pill');
    const parentBatchPillText = document.getElementById('inspect-parent-batch-pill-text');
    const parentBatchBtn = document.getElementById('inspect-parent-batch-btn');
    const parentBatchTitle = document.getElementById('inspect-parent-batch-title');

    if (parentBatch) {
        if (parentBatchPill) parentBatchPill.classList.remove('hidden');
        if (parentBatchPillText) parentBatchPillText.innerText = `${parentBatch.id.toUpperCase()}`;
        if (parentBatchBtn) parentBatchBtn.classList.remove('hidden');
        if (parentBatchTitle) parentBatchTitle.innerText = `${parentBatch.id.toUpperCase()}: ${parentBatch.title}`;
    } else {
        if (parentBatchPill) parentBatchPill.classList.add('hidden');
        if (parentBatchBtn) parentBatchBtn.classList.add('hidden');
    }

    updateInspectModalWatchlistBtn();
    setReviewerRecommendation('approve');

    // TOOL 1: Hotspots & Conflicts
    const conflictBadge = document.getElementById('inspect-conflict-status');
    const conflictDesc = document.getElementById('inspect-conflict-desc');
    const hotspotFilesContainer = document.getElementById('inspect-hotspot-files');

    if (pr.hotspots && pr.hotspots.length > 0) {
        conflictBadge.className = 'px-2 py-0.5 rounded text-[10px] font-semibold bg-rose-950 text-rose-300 border border-rose-800';
        conflictBadge.innerText = `🔥 ${pr.hotspots.length} Hotspot collisions`;
        conflictDesc.innerText = 'This PR modifies files that are simultaneously touched by multiple open PRs. Watch out for merge conflicts!';
        
        hotspotFilesContainer.innerHTML = pr.hotspots.map(h => {
            const competing = state.allPRs.filter(other => other.number !== pr.number && other.files && other.files.includes(h)).length;
            return `
                <div class="flex items-center justify-between p-2 rounded bg-rose-950/40 border border-rose-900/60 font-mono text-[11px] text-rose-200">
                    <span class="truncate">${escapeHtml(h)}</span>
                    <span class="ml-2 px-1.5 py-0.5 rounded bg-rose-800 text-white font-sans text-[10px] whitespace-nowrap">
                        ${competing} conflicting PRs
                    </span>
                </div>
            `;
        }).join('');
    } else {
        conflictBadge.className = 'px-2 py-0.5 rounded text-[10px] font-semibold bg-emerald-950 text-emerald-300 border border-emerald-800';
        conflictBadge.innerText = `🟢 Clean (0 conflicts)`;
        conflictDesc.innerText = 'This PR does not touch any of the 46 high-collision hotspot files in AnyPS5. Safe to merge independently!';
        hotspotFilesContainer.innerHTML = '';
    }

    // TOOL 2: CLI Commands
    document.getElementById('inspect-cmd-checkout').innerText = `gh pr checkout ${pr.number}`;
    document.getElementById('inspect-cmd-diff').innerText = `gh pr diff ${pr.number}`;
    document.getElementById('inspect-cmd-merge').innerText = isM 
        ? `gh pr merge ${pr.number} --squash --delete-branch`
        : `gh pr checkout ${pr.number} && cmake --build build --config Release`;

    // TOOL 3: Checklist
    const chk = pr.checklist || {};
    document.getElementById('chk-build').checked = !!chk.build;
    document.getElementById('chk-types').checked = !!chk.types;
    document.getElementById('chk-locks').checked = !!chk.locks;
    document.getElementById('chk-tests').checked = !!chk.tests;

    // TOOL 4: Notes
    document.getElementById('inspect-notes-field').value = pr.notes || '';

    // TOOL 5: Files List
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

function updateInspectModalWatchlistBtn() {
    const btn = document.getElementById('inspect-toggle-watchlist-btn');
    if (!btn || !state.activeInspectPR) return;
    const isWatched = state.reviewerWatchlist.has(state.activeInspectPR.number);
    btn.innerText = isWatched ? '✓ Remove from My Reviews' : '+ Add to My Reviews';
    btn.className = isWatched 
        ? 'px-3 py-1.5 bg-rose-950/60 hover:bg-rose-900 text-rose-300 border border-rose-800/80 rounded-lg text-xs font-semibold transition cursor-pointer'
        : 'px-3 py-1.5 bg-purple-950/60 hover:bg-purple-900 text-purple-300 border border-purple-800/80 rounded-lg text-xs font-semibold transition cursor-pointer';
}

function toggleCurrentPRWatchlist() {
    if (!state.activeInspectPR) return;
    const num = state.activeInspectPR.number;
    toggleWatchlistSingle(num);
    updateInspectModalWatchlistBtn();
}

function toggleWatchlistSingle(num) {
    if (state.reviewerWatchlist.has(num)) {
        state.reviewerWatchlist.delete(num);
        showToast(`PR #${num} removed from your review workbench`);
    } else {
        state.reviewerWatchlist.add(num);
        showToast(`PR #${num} added to your review workbench`);
    }
    savePersistentUser();
    renderMyPRs();
}

function setReviewerRecommendation(rec) {
    state.activeReviewerRecommendation = rec;
    ['approve', 'changes', 'issue'].forEach(r => {
        const btn = document.getElementById(`rec-btn-${r}`);
        if (btn) {
            if (r === rec) {
                btn.className = 'py-1.5 px-2 rounded-lg bg-cyan-950 border border-cyan-500 text-cyan-200 font-bold transition text-center text-[11px] shadow-sm cursor-pointer';
            } else {
                btn.className = 'py-1.5 px-2 rounded-lg bg-slate-950 hover:bg-slate-900 border border-slate-800 text-slate-400 font-semibold transition text-center text-[11px] cursor-pointer';
            }
        }
    });
}

function copyGeneratedMarkdownReview() {
    if (!state.activeInspectPR) return;
    const pr = state.activeInspectPR;
    const chk = getChecklistState();
    const notes = document.getElementById('inspect-notes-field')?.value || '';
    const rec = state.activeReviewerRecommendation || 'approve';

    const recLabels = {
        'approve': '✅ Verified & Recommended for Merge (LGTM)',
        'changes': '⚠️ Requested revisions before merge',
        'issue': '🚫 Issue / bug detected in code'
    };

    let md = `### 🔍 AnyPS5 Code Review\n\n`;
    md += `**Reviewer:** @${state.currentUser.username} (${isCurrentUserMaintainer() ? '🛡️ Maintainer' : '🔍 Community Reviewer'})\n`;
    md += `**Recommendation:** ${recLabels[rec] || recLabels.approve}\n\n`;
    md += `#### 📋 AnyPS5 Quality Standards Checklist:\n`;
    md += `- [${chk.build ? 'x' : ' '}] Build compiles cleanly (CMakeLists / Clang / MSVC)\n`;
    md += `- [${chk.types ? 'x' : ' '}] Verified SCE firmware signatures & struct definitions\n`;
    md += `- [${chk.locks ? 'x' : ' '}] No race conditions / thread locks properly released\n`;
    md += `- [${chk.tests ? 'x' : ' '}] Tests & regression checks included\n\n`;

    if (notes.trim()) {
        md += `#### 📝 Testing Notes:\n${notes.trim()}\n\n`;
    }

    md += `*Generated via [AnyPS5 Maintainer Workspace](https://ivica00.github.io/anyps5-maintainer-hub/)*\n`;

    copyTextToClipboard(md, 'Review copied in Markdown format!');
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
    renderMinibatches();
    renderRepoTable();

    await updatePRInSupabase(num, {
        status: state.activeInspectPR.status,
        notes: notes,
        checklist: checklist
    });
    showToast(`Saved notes for PR #${num}`);
}

function unassignCurrentPR() {
    if (!state.activeInspectPR) return;
    const num = state.activeInspectPR.number;
    closeInspectModal();
    unassignPR(num);
}


// ==========================================
// VIEW 3: CIJELI REPOZITORIJ (REPO TABLE)
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

    const isM = isCurrentUserMaintainer();

    tbody.innerHTML = list.slice(0, 50).map(pr => {
        const isAssignedToMe = pr.assignedReviewer === state.currentMaintainer;
        const isWatched = state.reviewerWatchlist.has(pr.number);
        const parentBatch = findBatchForPR(pr.number);

        const batchActionBtn = parentBatch ? `
            <button onclick="jumpToBatch('${parentBatch.id}')" title="View ${escapeHtml(parentBatch.title)} in Minibatch Studio"
                    class="px-2.5 py-1 bg-slate-900 hover:bg-cyan-950 text-cyan-400 border border-slate-700 hover:border-cyan-600 rounded text-[11px] font-mono font-semibold transition cursor-pointer flex items-center gap-1">
                <span>📦</span> ${parentBatch.id.toUpperCase()}
            </button>
        ` : `<span class="text-slate-500 font-mono text-[10px]">No batch</span>`;

        return `
            <tr class="hover:bg-slate-800/50 transition">
                <td class="px-4 py-2.5 font-mono font-bold text-cyan-400">#${pr.number}</td>
                <td class="px-4 py-2.5 max-w-sm truncate" title="${escapeHtml(pr.title)}">
                    <span class="text-slate-200 font-medium">${escapeHtml(pr.title)}</span>
                    <div class="text-[10px] text-slate-400 font-mono">by @${escapeHtml(pr.author)}</div>
                </td>
                <td class="px-4 py-2.5 whitespace-nowrap">${getCategoryBadge(pr.category)}</td>
                <td class="px-4 py-2.5 whitespace-nowrap">${getRiskBadge(pr.riskLevel)}</td>
                <td class="px-4 py-2.5 font-mono text-slate-300 whitespace-nowrap">
                    ${isAssignedToMe ? '<span class="text-cyan-400 font-bold">👤 You (@' + state.currentMaintainer + ')</span>' : (pr.assignedReviewer !== 'none' ? '@' + pr.assignedReviewer : '<span class="text-slate-500">Available</span>')}
                </td>
                <td class="px-4 py-2.5 text-right whitespace-nowrap">
                    <div class="inline-flex items-center gap-1.5 justify-end">
                        <button onclick="openInspectModal(${pr.number})" class="px-2.5 py-1 bg-cyan-950 hover:bg-cyan-900 text-cyan-300 border border-cyan-800 rounded font-semibold text-[11px] transition cursor-pointer">
                            🧪 Inspect
                        </button>
                        ${batchActionBtn}
                    </div>
                </td>
            </tr>
        `;
    }).join('');
}

// ==========================================
// GITHUB REST API CLIENT & AUTHENTICATION
// ==========================================

const GitHubClient = {
    repoOwner: 'boykopovar',
    repoName: 'AnyPS5',

    getHeaders(token = state.currentUser?.token) {
        const headers = {
            'Accept': 'application/vnd.github.v3+json'
        };
        if (token) {
            headers['Authorization'] = `token ${token}`;
        }
        return headers;
    },

    async fetchPublicProfile(username) {
        try {
            const res = await fetch(`https://api.github.com/users/${encodeURIComponent(username)}`, {
                headers: this.getHeaders()
            });
            if (!res.ok) throw new Error(`HTTP ${res.status}`);
            return await res.json();
        } catch (e) {
            console.warn('fetchPublicProfile error:', e);
            return null;
        }
    },

    async fetchAuthenticatedUser(token) {
        try {
            const res = await fetch('https://api.github.com/user', {
                headers: this.getHeaders(token)
            });
            if (!res.ok) throw new Error(`HTTP ${res.status}`);
            return await res.json();
        } catch (e) {
            console.warn('fetchAuthenticatedUser error:', e);
            return null;
        }
    },

    async checkRepoPermissions(token) {
        try {
            const res = await fetch(`https://api.github.com/repos/${this.repoOwner}/${this.repoName}`, {
                headers: this.getHeaders(token)
            });
            if (!res.ok) throw new Error(`HTTP ${res.status}`);
            const data = await res.json();
            return data.permissions || null;
        } catch (e) {
            console.warn('checkRepoPermissions error:', e);
            return null;
        }
    },

    async fetchLivePR(prNumber) {
        try {
            const res = await fetch(`https://api.github.com/repos/${this.repoOwner}/${this.repoName}/pulls/${prNumber}`, {
                headers: this.getHeaders()
            });
            if (!res.ok) throw new Error(`HTTP ${res.status}`);
            return await res.json();
        } catch (e) {
            console.warn('fetchLivePR error:', e);
            return null;
        }
    },

    async postPRComment(prNumber, commentBody, token = state.currentUser?.token) {
        if (!token) {
            throw new Error('No GitHub token provided. Please sign in with GitHub.');
        }
        const res = await fetch(`https://api.github.com/repos/${this.repoOwner}/${this.repoName}/issues/${prNumber}/comments`, {
            method: 'POST',
            headers: {
                ...this.getHeaders(token),
                'Content-Type': 'application/json'
            },
            body: JSON.stringify({ body: commentBody })
        });
        if (!res.ok) {
            const errJson = await res.json().catch(() => ({}));
            throw new Error(errJson.message || `GitHub API error HTTP ${res.status}`);
        }
        return await res.json();
    }
};

// Global Supabase client instance
let sbClientInstance = null;
function getSupabaseClient() {
    if (!sbClientInstance && window.supabase && typeof window.supabase.createClient === 'function') {
        sbClientInstance = window.supabase.createClient(SUPABASE_URL, SUPABASE_ANON_KEY);
    }
    return sbClientInstance;
}

// ==========================================
// OFFICIAL GITHUB OAUTH 2.0 FLOW
// ==========================================

async function initSupabaseAuth() {
    const sb = getSupabaseClient();
    if (!sb) return;

    try {
        const { data: { session } } = await sb.auth.getSession();
        if (session && session.user) {
            handleOAuthUserSession(session);
        }
    } catch (e) {
        console.warn('Supabase getSession error:', e);
    }

    sb.auth.onAuthStateChange((event, session) => {
        if (event === 'SIGNED_IN' && session && session.user) {
            handleOAuthUserSession(session);
        } else if (event === 'SIGNED_OUT') {
            state.currentUser = null;
            state.currentMaintainer = '';
            savePersistentUser();
            updateUserUI();
            renderMyPRs();
            renderMinibatches();
            renderRepoTable();
        }
    });
}

function handleOAuthUserSession(session) {
    if (!session || !session.user) return;
    const user = session.user;
    const meta = user.user_metadata || {};
    const username = meta.user_name || meta.preferred_username || user.email?.split('@')[0] || 'github_user';
    const avatar = meta.avatar_url || `https://github.com/${username}.png`;
    const name = meta.full_name || meta.name || username;
    const providerToken = session.provider_token || state.currentUser?.token || '';

    const isMaintainer = KNOWN_MAINTAINERS.map(m => m.toLowerCase()).includes(username.toLowerCase());

    state.currentUser = {
        username: username,
        name: name,
        avatar: avatar,
        role: isMaintainer ? 'maintainer' : 'reviewer',
        token: providerToken
    };
    state.currentMaintainer = username;

    savePersistentUser();
    updateUserUI();
    renderMyPRs();
    renderMinibatches();
    renderRepoTable();

    // Clean up URL hash tokens after redirect
    if (window.location.hash && (window.location.hash.includes('access_token') || window.location.hash.includes('error'))) {
        try {
            history.replaceState(null, '', window.location.pathname + window.location.search);
        } catch (e) {}
    }

    showToast(`Signed in with GitHub: @${username} (${isMaintainer ? 'Maintainer' : 'Reviewer'})`);
}

// Initiate official GitHub OAuth redirect (Authorize application)
async function loginWithGitHubOAuth() {
    const sb = getSupabaseClient();
    if (!sb) {
        showToast('Supabase authentication is not ready');
        return;
    }

    const btn = document.getElementById('btn-github-oauth-login');
    const originalText = btn ? btn.innerHTML : '';
    if (btn) {
        btn.disabled = true;
        btn.innerHTML = `<span class="animate-spin inline-block">⏳</span> Redirecting to GitHub...`;
    }

    try {
        const redirectUrl = window.location.origin.includes('localhost') 
            ? window.location.origin + window.location.pathname 
            : 'https://ivica00.github.io/anyps5-maintainer-hub/';
        const { data, error } = await sb.auth.signInWithOAuth({
            provider: 'github',
            options: {
                redirectTo: redirectUrl,
                scopes: 'read:user repo'
            }
        });

        if (error) {
            console.error('Supabase signInWithOAuth error:', error);
            if (btn) {
                btn.disabled = false;
                btn.innerHTML = originalText;
            }
            if (error.message && (error.message.includes('not enabled') || error.message.includes('Unsupported provider'))) {
                const notice = document.getElementById('oauth-setup-notice');
                if (notice) notice.classList.remove('hidden');
                showToast('GitHub OAuth provider needs to be enabled in Supabase');
            } else {
                showToast(`Error: ${error.message}`);
            }
        }
    } catch (err) {
        console.error('OAuth redirect failed:', err);
        if (btn) {
            btn.disabled = false;
            btn.innerHTML = originalText;
        }
        showToast(`Error: ${err.message}`);
    }
}

async function signOutGitHub() {
    const sb = getSupabaseClient();
    if (sb) {
        try {
            await sb.auth.signOut();
        } catch (e) {
            console.warn('SignOut error:', e);
        }
    }
    state.currentUser = null;
    state.currentMaintainer = '';
    savePersistentUser();
    updateUserUI();
    renderMyPRs();
    renderMinibatches();
    renderRepoTable();
    closeGitHubLoginModal();
    showToast('Signed out from GitHub');
}

function openGitHubLoginModal() {
    const modal = document.getElementById('github-login-modal');
    if (!modal) return;
    updateUserUI();
    modal.classList.remove('hidden');
    modal.classList.add('flex');
}

function closeGitHubLoginModal() {
    const modal = document.getElementById('github-login-modal');
    if (modal) {
        modal.classList.add('hidden');
        modal.classList.remove('flex');
    }
}

// 🚀 1-Click Post review directly to GitHub PR via API
async function postReviewToGitHubViaAPI() {
    if (!state.activeInspectPR) return;
    const pr = state.activeInspectPR;

    if (!state.currentUser || !state.currentUser.token) {
        const proceed = confirm(`Directly posting a review to the AnyPS5 repository requires an active GitHub session with repository access.\n\nWould you like to authorize with GitHub now?`);
        if (proceed) {
            openGitHubLoginModal();
        }
        return;
    }

    const chk = getChecklistState();
    const notes = document.getElementById('inspect-notes-field')?.value || '';
    const rec = state.activeReviewerRecommendation || 'approve';

    const recLabels = {
        'approve': '✅ Verified & Recommended for Merge (LGTM)',
        'changes': '⚠️ Suggested revisions before merging',
        'issue': '🚫 Issue / Bug detected in code'
    };

    let md = `### 🔍 AnyPS5 Code Review\n\n`;
    md += `**Reviewer:** @${state.currentUser.username} (${isCurrentUserMaintainer() ? '🛡️ Maintainer' : '🔍 Community Reviewer'})\n`;
    md += `**Recommendation:** ${recLabels[rec] || recLabels.approve}\n\n`;
    md += `#### 📋 AnyPS5 Quality Standards Checklist:\n`;
    md += `- [${chk.build ? 'x' : ' '}] Build compiles cleanly (CMakeLists / Clang / MSVC)\n`;
    md += `- [${chk.types ? 'x' : ' '}] Verified SCE firmware signatures & struct definitions\n`;
    md += `- [${chk.locks ? 'x' : ' '}] No race conditions / thread locks properly released\n`;
    md += `- [${chk.tests ? 'x' : ' '}] Tests & regression checks included\n\n`;

    if (notes.trim()) {
        md += `#### 📝 Testing Notes:\n${notes.trim()}\n\n`;
    }

    md += `*Posted directly via [AnyPS5 Maintainer Workspace](https://ivica00.github.io/anyps5-maintainer-hub/) with GitHub OAuth integration.*`;

    const btn = document.getElementById('btn-api-submit');
    const originalText = btn ? btn.innerHTML : '';
    if (btn) {
        btn.disabled = true;
        btn.innerHTML = `<span class="animate-spin inline-block">⏳</span> Publishing to GitHub...`;
    }

    try {
        await GitHubClient.postPRComment(pr.number, md, state.currentUser.token);
        showToast(`🚀 Review successfully posted to GitHub for PR #${pr.number}!`);
        await saveInspectionAndClose();
    } catch (err) {
        console.error('postReviewToGitHubViaAPI error:', err);
        showToast(`❌ Error posting review: ${err.message}`);
        if (btn) {
            btn.disabled = false;
            btn.innerHTML = originalText;
        }
    }
}

function handleHeroAction() {
    if (!state.currentUser) {
        openGitHubLoginModal();
        return;
    }
    switchMainView('batches');
}

function addSafeQuickBatchToReviewer() {
    const safeBatch = state.minibatches.find(b => b.complexity === 'low') || state.minibatches[0];
    if (safeBatch) {
        addBatchToReviewWatchlist(safeBatch.id);
    }
}

async function quickClaimSafeBatch() {
    if (!isCurrentUserMaintainer()) {
        addSafeQuickBatchToReviewer();
        return;
    }
    const safeBatch = state.minibatches.find(b => (b.status === 'available' || b.claimedBy === 'none') && b.complexity === 'low')
        || state.minibatches.find(b => b.status === 'available' || b.claimedBy === 'none');
    
    if (safeBatch) {
        await claimBatchServerSide(safeBatch.id);
        switchMainView('mine');
    } else {
        showToast('No available batches found in database');
    }
}

// Switch between "My PRs", "Minibatches", and "Entire Repository"
function switchMainView(view) {
    state.activeView = view;
    const btnMine = document.getElementById('view-tab-mine');
    const btnBatches = document.getElementById('view-tab-batches');
    const btnRepo = document.getElementById('view-tab-repo');

    const secMine = document.getElementById('view-mine-section');
    const secBatches = document.getElementById('view-batches-section');
    const secRepo = document.getElementById('view-repo-section');

    // Reset tab active styling
    [btnMine, btnBatches, btnRepo].forEach(btn => {
        if (btn) {
            btn.classList.remove('active', 'border-cyan-400', 'text-cyan-300');
            btn.classList.add('text-slate-400', 'border-transparent');
        }
    });

    [secMine, secBatches, secRepo].forEach(sec => {
        if (sec) sec.classList.add('hidden');
    });

    if (view === 'mine') {
        btnMine.classList.add('active', 'border-cyan-400', 'text-cyan-300');
        btnMine.classList.remove('text-slate-400', 'border-transparent');
        secMine.classList.remove('hidden');
        renderMyPRs();
    } else if (view === 'batches') {
        btnBatches.classList.add('active', 'border-cyan-400', 'text-cyan-300');
        btnBatches.classList.remove('text-slate-400', 'border-transparent');
        secBatches.classList.remove('hidden');
        renderMinibatches();
    } else {
        btnRepo.classList.add('active', 'border-cyan-400', 'text-cyan-300');
        btnRepo.classList.remove('text-slate-400', 'border-transparent');
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
        return `<span class="inline-flex items-center gap-1 px-1.5 py-0.5 rounded-full text-[10px] font-semibold bg-amber-950 text-amber-300 border border-amber-800 whitespace-nowrap">🟡 Medium Risk</span>`;
    } else {
        return `<span class="inline-flex items-center gap-1 px-1.5 py-0.5 rounded-full text-[10px] font-bold bg-rose-950 text-rose-300 border border-rose-800 whitespace-nowrap">🔴 Hotspot Risk</span>`;
    }
}

function getCategoryBadge(cat) {
    return `<span class="px-1.5 py-0.5 rounded text-[10px] font-semibold bg-slate-800 text-slate-300 border border-slate-700 whitespace-nowrap">${escapeHtml(cat)}</span>`;
}

function getStatusBadge(status) {
    const badges = {
        'unassigned': '<span class="text-slate-400 bg-slate-900 border border-slate-800 px-1.5 py-0.5 rounded text-[10px] whitespace-nowrap">📥 Unassigned</span>',
        'in-review': '<span class="text-sky-300 bg-sky-950 border border-sky-800 px-1.5 py-0.5 rounded text-[10px] whitespace-nowrap">🔍 In Review</span>',
        'approved': '<span class="text-emerald-300 bg-emerald-950 border border-emerald-800 px-1.5 py-0.5 rounded text-[10px] whitespace-nowrap">✅ Approved</span>',
        'changes': '<span class="text-amber-300 bg-amber-950 border border-amber-800 px-1.5 py-0.5 rounded text-[10px] whitespace-nowrap">⚠️ Changes Requested</span>',
        'conflict': '<span class="text-rose-300 bg-rose-950 border border-rose-800 px-1.5 py-0.5 rounded text-[10px] whitespace-nowrap">🚫 Conflict</span>',
        'ready': '<span class="text-cyan-300 bg-cyan-950 border border-cyan-800 px-1.5 py-0.5 rounded text-[10px] font-bold whitespace-nowrap">🚀 Ready to Merge</span>'
    };
    return badges[status] || badges['unassigned'];
}

function copyCliCheckout(prNumber) {
    const cmd = `gh pr checkout ${prNumber}`;
    copyTextToClipboard(cmd, `Copied: ${cmd}`);
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
            prompt('Copy command:', text);
        });
    } else {
        prompt('Copy command:', text);
    }
}

function formatDate(isoStr) {
    if (!isoStr) return '';
    try {
        const d = new Date(isoStr);
        return d.toLocaleDateString('en-US', { month: 'short', day: 'numeric', year: 'numeric' });
    } catch (e) {
        return isoStr;
    }
}

function formatRelativeTime(isoStr) {
    if (!isoStr) return '';
    try {
        const diff = (new Date() - new Date(isoStr)) / 1000;
        if (diff < 60) return 'just now';
        if (diff < 3600) return `${Math.floor(diff / 60)}m ago`;
        if (diff < 86400) return `${Math.floor(diff / 3600)}h ago`;
        return `${Math.floor(diff / 86400)}d ago`;
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

// ==========================================
// PLAYSTATION 5 LIVE INTELLIGENCE SIDEBAR
// ==========================================

function renderSidebarStats() {
    const totalEl = document.getElementById('sidebar-total-prs');
    const lowCountEl = document.getElementById('sidebar-low-count');
    const medCountEl = document.getElementById('sidebar-med-count');
    const highCountEl = document.getElementById('sidebar-high-count');
    const barLow = document.getElementById('sidebar-bar-low');
    const barMed = document.getElementById('sidebar-bar-med');
    const barHigh = document.getElementById('sidebar-bar-high');

    const total = state.allPRs.length || 0;
    if (totalEl) totalEl.innerText = total;

    let lowCount = 0;
    let medCount = 0;
    let highCount = 0;

    const fileMap = {};

    state.allPRs.forEach(pr => {
        if (pr.riskLevel === 'low') lowCount++;
        else if (pr.riskLevel === 'high') highCount++;
        else medCount++;

        if (Array.isArray(pr.hotspots)) {
            pr.hotspots.forEach(f => {
                if (!f) return;
                fileMap[f] = (fileMap[f] || 0) + 1;
            });
        }
    });

    if (lowCountEl) lowCountEl.innerText = lowCount;
    if (medCountEl) medCountEl.innerText = medCount;
    if (highCountEl) highCountEl.innerText = highCount;

    if (total > 0) {
        const lowPct = Math.round((lowCount / total) * 100);
        const medPct = Math.round((medCount / total) * 100);
        const highPct = Math.max(0, 100 - lowPct - medPct);
        if (barLow) barLow.style.width = `${lowPct}%`;
        if (barMed) barMed.style.width = `${medPct}%`;
        if (barHigh) barHigh.style.width = `${highPct}%`;
    }

    // Maintainers Roster with live workload
    const maintainersListEl = document.getElementById('sidebar-maintainers-list');
    if (maintainersListEl) {
        maintainersListEl.innerHTML = KNOWN_MAINTAINERS.map(m => {
            const assignedCount = state.allPRs.filter(p => p.assignedReviewer === m).length;
            const isMe = m === state.currentMaintainer;
            return `
                <div class="flex items-center justify-between p-2 rounded-xl ${isMe ? 'bg-blue-950/50 border border-blue-500/50 shadow-sm' : 'bg-slate-900/60 border border-slate-800/80'} text-xs transition">
                    <div class="flex items-center gap-2">
                        <img src="https://github.com/${m}.png" alt="${m}" class="w-5 h-5 rounded-full ring-1 ring-slate-700 flex-shrink-0" onerror="this.src='https://github.com/ghost.png'">
                        <span class="font-mono text-xs ${isMe ? 'text-cyan-300 font-bold' : 'text-slate-300'}">@${m}</span>
                        ${isMe ? '<span class="text-[9px] px-1.5 py-0.2 rounded-full bg-blue-900/80 text-cyan-200 border border-blue-700 font-semibold">You</span>' : ''}
                    </div>
                    <span class="text-[10px] font-mono px-2 py-0.5 rounded-full ${assignedCount > 0 ? 'bg-blue-900/60 text-cyan-200 border border-blue-700/60 font-bold' : 'bg-slate-950 text-slate-500 border border-slate-800'}">
                        ${assignedCount} PR
                    </span>
                </div>
            `;
        }).join('');
    }

    // Top Conflict Hotspots Radar
    const hotspotsListEl = document.getElementById('sidebar-hotspots-list');
    if (hotspotsListEl) {
        const sortedHotspots = Object.entries(fileMap)
            .sort((a, b) => b[1] - a[1])
            .slice(0, 4);

        if (sortedHotspots.length === 0) {
            hotspotsListEl.innerHTML = `
                <div class="text-[11px] text-slate-500 italic p-2 text-center">
                    No hotspot collisions detected in repository
                </div>
            `;
        } else {
            hotspotsListEl.innerHTML = sortedHotspots.map(([file, count]) => {
                const shortName = file.split('/').pop();
                return `
                    <div class="p-2 rounded-xl bg-slate-900/80 border border-rose-950/60 flex items-center justify-between gap-2 hover:border-rose-900 transition">
                        <div class="truncate">
                            <div class="font-mono text-rose-300 font-semibold truncate text-[11px]" title="${escapeHtml(file)}">
                                ${escapeHtml(shortName)}
                            </div>
                            <div class="text-[9px] text-slate-500 truncate" title="${escapeHtml(file)}">
                                ${escapeHtml(file)}
                            </div>
                        </div>
                        <span class="px-2 py-0.5 rounded-full bg-rose-950/90 border border-rose-800 text-rose-300 text-[10px] font-mono font-bold whitespace-nowrap">
                            ${count} PR
                        </span>
                    </div>
                `;
            }).join('');
        }
    }
}

