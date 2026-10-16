# AnyPS5 Maintainer Hub & PR Coordination Center

Interaktivna web aplikacija i organizacijska platforma za maintainere repozitorija [`boykopovar/AnyPS5`](https://github.com/boykopovar/AnyPS5).

Ovaj alat rješava problem velikog priljeva pull requestova (~800 otvorenih PR-ova) automatskom kategorizacijom podsustava, ranim otkrivanjem *merge* konflikata na „hotspot” datotekama, te omogućavanjem sekvencijalnog paralelnog spajanja (*conflict-free batch merge*).

---

## 🚀 Brzo pokretanje

### Opcija 1: Izravno otvaranje (Bez servera)
Jednostavno dvokliknite na [`index.html`](index.html). Stranica radi 100% lokalno i offline jer je cjelokupni skup podataka svih PR-ova već predmemoriran unutar [`prs_data.js`](prs_data.js).

### Opcija 2: Pokretanje lokalnog servera
U PowerShell ili terminalu pokrenite:
```powershell
python server.py
```
Otvorit će se u pregledniku na adresi: `http://localhost:8000/index.html`.

---

## 🎯 Ključne značajke i mogućnosti

### 1. 📋 Triage Matrix (Glavna filtracijska tablica)
- **Automatska klasifikacija u 7 podsustava:**
  - 🎮 **AGC & Graphics Driver** (`libSceAgcDriver`, Vulkan device, draw, textures)
  - 📦 **PRX High-Level Modules** (`libSceVideoOut`, `libSceNet`, `libScePad`, `libSceSaveData`, `libSceAudioOut`...)
  - 🧠 **Kernel & Syscalls** (`libkernel`, virtual memory, kqueue, sysconf)
  - ⚡ **Shader Recompiler & Relinker** (`core/shader`, `core/relinker`)
  - 🧱 **LibC & Base Runtime** (`libc`, `libm`, `libunwind`, iznimke)
  - 🛠️ **Build & Tooling** (CMake konfiguracija, testovi, alati)
  - 📖 **Documentation & Debt**
- **Semafor rizika od konflikta:**
  - 🟢 **Quick Merge (Safe)**: $\le 3$ datoteke, $< 300$ linija koda, **0 hotspot datoteka**. Idealan kandidat za trenutačno spajanje.
  - 🟡 **Moderate Risk**: Veći zahvat ($300 - 1500$ linija koda ili 4–10 datoteka).
  - 🔴 **Hotspot Collision**: Dira barem jednu od 46 najfrekventnijih datoteka u repozitoriju (npr. `CMakeLists.txt`, `libSceAgcDriver.md`, `Graphics.cpp`).
- **Višestruki odabir (Batch Selection)** za generiranje skripti.

### 2. 📌 Kanban Board
- Praćenje stanja recenzije:
  - 📥 *Incoming* (Unassigned)
  - 🔍 *In Review*
  - ✅ *Approved*
  - ⚠️ *Changes Requested*
  - 🚫 *Conflict Blocked*
  - 🚀 *Ready to Merge*
- **Trajna pohrana:** Stanje, zabilješke i dodijeljeni maintaineri automatski se spremaju u `localStorage` vašeg preglednika i ostaju sačuvani i nakon osvježavanja stranice.

### 3. 🎯 Hotspot Collision Radar
- Prikaz 46 najkolizijskih datoteka u repozitoriju (npr. `libSceAgcDriver/CMakeLists.txt` koji dira čak 96 otvorenih PR-ova!).
- Jednim klikom filtrira sve konkurentne PR-ove koji se natječu za istu datoteku.

### 4. 👥 Reviewers & Workload Balancer
- Pregled opterećenja maintainera (`boykopovar`, `Honza11`, `Eljack`, `JJenkx`, `MarcZX100`, `DavidSamir`).
- Mogućnost brze raspodjele filtriranog seta PR-ova određenom recenzentu.

### 5. ⚡ Conflict-Free Batch Merge Planner
- Algoritam automatski pronalazi skup PR-ova koji diraju **potpuno disjunktne skupove datoteka** i ne diraju hotspotove.
- Generira gotovu GitHub CLI shell skriptu:
  ```bash
  gh pr merge 6836 --repo boykopovar/AnyPS5 --auto --squash --delete-branch
  gh pr merge 6834 --repo boykopovar/AnyPS5 --auto --squash --delete-branch
  ```
  Koju možete kopirati jednim klikom i pokrenuti u terminalu za spajanje 10–20 PR-ova u sekundi bez ikakvog merge konflikta!

### 6. 🔍 Detaljni PR Drawer (Modal)
- Klikom na bilo koji redak otvara se prozor s:
  - Prikazom autora, avatara i datuma.
  - Direktnim GitHub CLI komandama: `gh pr checkout <num>`, `gh pr diff <num>`, `gh pr merge <num>`.
  - Popisom svih izmijenjenih datoteka s jasnim crvenim oznakama ako datoteka predstavlja hotspot.
  - Poljem za maintainer zabilješke (notes).

---

## 🔄 Osvježavanje podataka

Skripta [`build_dataset.py`](build_dataset.py) može se pokrenuti u bilo kojem trenutku za osvježavanje lokalne baze:
```powershell
python build_dataset.py
```
Također, unutar same aplikacije nalazi se gumb **"Live GitHub Sync"** koji može dohvatiti najnovije PR-ove izravno putem GitHub API-ja (uz podršku za opcionalni GitHub Personal Access Token radi izbjegavanja rate-limita).
