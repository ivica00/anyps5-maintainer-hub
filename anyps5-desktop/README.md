# 🦀 AnyPS5 Maintainer Workspace - 100% Pure Rust Desktop

Potpuno **nativna Rust GUI aplikacija** izrađena pomoću [`eframe`](https://github.com/emilk/egui/tree/master/crates/eframe) i [`egui`](https://github.com/emilk/egui).

**Bez HTML-a. Bez CSS-a. Bez JavaScripta. Bez WebView/Chromium omotača.**  
Grafičko sučelje se iscrtava izravno na GPU-u (preko DirectX/wgpu na Windowsima), a podaci se vuku izravno iz **Supabase PostgreSQL** baze kroz nativni Rust HTTP klijent (`ureq`).

---

## ⚡ Arhitektura sustava

```
[ Supabase PostgreSQL ] <─── ureq (Rust REST API) ───┐
                                                     │
                                           [ anyps5-desktop ]
                                                     │
[ DirectX / wgpu Hardware Rendering ] <─── eframe / egui GUI ────┘
                                                     │
                                                     ├── ⚡ Minibatches & Triage Matrix
                                                     ├── 📋 AI Prompt Generator (Claude/GPT)
                                                     ├── 💻 Live Hardware & C++ Toolchain
                                                     └── 🚀 GitHub Actions Runner Orchestrator
```

---

## 🎯 Mogućnosti nativne aplikacije

1. **⚡ Minibatches Tab:**
   - Dohvaća nekonfliktne pakete PR-ova sa Supabasea u pozadinskom threadu.
   - Prikazuje kategorije, oznake složenosti (Safe, Moderate, High) i uključene PR brojeve.
   - Pretraživanje i filtriranje u real-timeu.

2. **📋 AI Batch Inspector & Prompt Generator:**
   - Detaljan pregled svih PR-ova u odabranom paketu (+/- linije koda, izmijenjene datoteke).
   - **Gumb "Copy AI Review Prompt":** Jednim klikom generira deterministički Markdown prompt i kopira ga u Windows clipboard.
   - **Gumb "Copy `gh` CLI Merge Script":** Generira GitHub CLI sekvencu za spajanje paketa.

3. **⚡ Lend CPU & Runner Hub:**
   - Prikaz stvarnih specifikacija računala: broj logičkih jezgri procesora (`thread::available_parallelism()`), OS i arhitektura.
   - Detekcija lokalnih alata: `git`, `cmake`, `ninja`, `clang++`.
   - **Native Build Worker:** Nije implementiran; sučelje to jasno označava. Za build poslove koristi se zaseban GitHub Actions runner.
   - **Launch GitHub Actions Runner:** Pokreće PowerShell runner servis u novom prozoru.

4. **🌐 Entire Repo (All PRs):**
   - Pregled i pretraga svih 800+ PR-ova izravno iz Supabase baze (uz offline fallback na `all_prs.json`).

---

## 🚀 Pokretanje

### Jednim klikom:
Dvokliknite na [`run_desktop.bat`](../run_desktop.bat) u korijenu repozitorija.

### Preko terminala:
```powershell
cd anyps5-desktop
cargo run
```

### Izgradnja optimiziranog Release `.exe` programa:
```powershell
cd anyps5-desktop
cargo build --release
```
Kompajlirani program nalazit će se u: `target/release/anyps5-desktop.exe`.

## Pouzdanost podataka

REST podaci se dohvaćaju po stranicama, uz timeout od 30 sekundi po zahtjevu.
Pri grešci prikazuje se status djelomičnih ili lokalnih podataka. Lokalni JSON i
runner mapa traže se uz izvršnu datoteku i u nadređenim mapama repozitorija.
Za samostalnu distribuciju priložite `all_prs.json` i mapu `runner` uz `.exe`.

Prompt i merge skripta blokirani su ako nedostaju PR-ovi ili putanje datoteka,
postoje duplikati PR brojeva ili se datoteke preklapaju. Provjera vrijedi za
učitani snapshot; aktualne GitHub diffove i CI treba provjeriti prije mergea.
Otvaranje runner setup prozora nije potvrda registracije ili aktivnosti runnera.

Provjere: `cargo test --locked --offline` i
`cargo clippy --locked --offline --all-targets -- -D warnings`.

## PS5 izgled

Sučelje koristi plavo-crnu pozadinu, prozračne panele, veću tipografiju i bijeli
fokus pri navigaciji tipkovnicom. Minibatches ima pretraživanje, filtre i sortiranje;
Inspector ima oblikovani pregled i Markdown, uz podtabove na manjim širinama.
Build Runner vodi kroz postavljanje, a Pull Requests zadržava virtualizaciju,
poravnate stupce, filtre i prozor s detaljima odabranog PR-a.

Test rasporeda provjerava širinu sadržaja sva četiri taba na 1000×640 i 1440×920,
uz ekvivalent skaliranja 100%, 125% i 150%, s dugim naslovima i putanjama.
Ovaj test ne zamjenjuje vizualnu provjeru stvarnog GPU prozora.

## GitHub račun

Podnožje prikazuje username i avatar aktivnog GitHub CLI računa. Ako račun nije
 dostupan, odaberite `Sign in with GitHub`. Potreban je instaliran `gh`; njegov
OAuth web postupak otvara se u zasebnom prozoru, a dovršavate ga u pregledniku.
Jednokratni kod CLI kopira u međuspremnik. Tokenima i njihovim spremanjem upravlja
GitHub CLI. `Log out` uklanja lokalnu GitHub CLI prijavu prikazanog računa; ne opoziva token na GitHubu.
Prijava na GitHub odvojena je od statusa veze s bazom podataka.

## Review workspace

Inspector opens a PR selection with Summary, Files, Review and Checks. The AI
prompt is a separate tab. Existing notes and checklist are loaded from Supabase;
only a Supabase-authenticated owner can edit and save. GitHub CLI login alone
is not a Supabase session. Claim and release use the existing protected RPCs.

`Sign in to review` uses GitHub through Supabase Auth with PKCE and an in-memory
session. The Supabase Auth redirect allowlist must include
`http://127.0.0.1:39187/**`; the GitHub provider must already be enabled. Login
expires after three minutes if no callback arrives. Sessions expire according to
the server and require another login; tokens are not persisted to disk.
This redirect/provider configuration and the interactive browser flow must be
verified on the deployed project before treating remote review actions as tested.

Saving checks ownership, head SHA and the original notes/checklist/status to avoid
overwriting a concurrent review. Decisions record `_reviewed_head_sha` inside the
checklist, which the existing sync clears on head changes. Unknown checklist keys
are preserved. Drafts remain in memory after failed saves or refreshes; closing
the application discards unsaved drafts. Copy notes before reloading a draft.

GitHub Checks refreshes on demand using CLI authentication, reads the current
PR head, check runs and commit statuses, and labels truncated check-run lists.
Results are tied to that SHA and show their age. They do not authorize merging.

### Checks details

Opening Checks fetches automatically once per PR/snapshot head; failures retain
previous results and offer Retry. Batch refresh fetches all members sequentially.
Check runs and commit statuses are paginated to completion; only the newest
commit status for each context is shown. Failed/running checks appear first,
successes can collapse, and skipped/unknown results remain separate.
Each result shows duration when available and a direct link supplied by GitHub.
The head is fetched again after collecting checks to detect concurrent pushes.
Results older than five minutes, failed refreshes and head mismatches are marked
stale. Required-check rules are not fetched: the UI explicitly says unknown and
never infers merge readiness. Browser sign-in can now be cancelled.

### Compact PR overview

PR selection uses two-line title previews and a compact metadata line. Detail
navigation uses underlined tabs, and the overview shows one line of diff stats.
Opening Overview fetches the PR description through GitHub CLI and caches it by
PR number and snapshot SHA. Empty descriptions, pending requests and failures
have distinct states; failed requests offer Retry. A changed remote head rejects
the response until the snapshot is refreshed. Headings and fenced code are
rendered locally with a selectable raw Markdown alternative; no embedded HTML or
remote images execute. Automated layout tests do not make network requests.
