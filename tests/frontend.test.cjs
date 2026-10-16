const fs = require('node:fs');
const vm = require('node:vm');
const assert = require('node:assert/strict');
const storage = new Map();
const context = vm.createContext({
    console, Set, Date, URL, setTimeout, clearTimeout,
    document: { addEventListener() {}, getElementById: () => ({ value: 'review notes', checked: false }) },
    window: {}, localStorage: {
        getItem: k => storage.get(k) || null,
        setItem: (k,v) => storage.set(k,v), removeItem: k => storage.delete(k)
    }
});
vm.runInContext(fs.readFileSync('app.js', 'utf8'), context);
(async () => {
    await assert.rejects(vm.runInContext("supabaseRequest('prs', {method:'PATCH'})", context), /Sign in/);
    vm.runInContext("sbClientInstance = {auth:{getSession:async()=>({data:{session:{access_token:'session-token'}}})}}", context);
    context.fetch = async (_url, options) => {
        assert.equal(options.headers.Authorization, 'Bearer session-token');
        return {ok:false, status:403};
    };
    await assert.rejects(vm.runInContext("supabaseRequest('prs', {method:'PATCH'})", context), /403/);
    storage.set('anyps5_auth_user', JSON.stringify({username:'forged',token:'secret'}));
    vm.runInContext('loadPersistentUser()', context);
    assert.equal(storage.has('anyps5_auth_user'), false);
    assert.equal(vm.runInContext('state.currentUser', context), null);
    vm.runInContext(`
        showToast = () => {};
        renderMyPRs = renderMinibatches = renderRepoTable = () => {};
        closeInspectModal = () => { state.activeInspectPR = null; };
        state.activeInspectPR = {number:42,status:'approved'};
    `, context);
    context.fetch = async () => ({ok:true, json:async()=>[{number:42}]});
    assert.equal(await vm.runInContext('saveInspectionAndClose()',context), true);
    assert.equal(vm.runInContext('state.activeInspectPR',context), null);
    vm.runInContext("state.activeInspectPR = {number:42,status:'in-review'}",context);
    context.fetch = async () => ({ok:false,status:403});
    assert.equal(await vm.runInContext('saveInspectionAndClose()',context), false);
    assert.equal(vm.runInContext('state.activeInspectPR.number',context),42);
    let pageCalls = 0;
    context.fetch = async (url) => {
        assert.ok(url.includes(`offset=${pageCalls}`));
        return {ok:true, json:async()=>pageCalls++ < 2 ? [{number:pageCalls}] : []};
    };
    const rows = await vm.runInContext("readAllSupabaseRows('prs?order=number.asc')",context);
    assert.equal(rows.length, 2);
    assert.equal(pageCalls, 3);
    vm.runInContext(`state.minibatches = [{status:'available',claimedBy:'none',prNumbers:[42],
        prs:[{githubState:'closed',files:['one.cpp']}]}]`,context);
    assert.equal(vm.runInContext('isBatchAvailable(state.minibatches[0])',context),false);
    vm.runInContext("state.allPRs = [{githubState:'open'},{githubState:'merged'},{githubState:'closed'}]",context);
    assert.equal(vm.runInContext('getOpenPRs().length',context),1);
    console.log('PASS: session authorization, rejected writes, forged identity cleanup, inspection save success/failure, capped pagination, closed batch exclusion');
})().catch(error => { console.error(error); process.exitCode=1; });
