# ⚡ AnyPS5 Community Builder & Self-Hosted Runner Hub

Dobrodošli u sustav distribuirane kompilacije za [`boykopovar/AnyPS5`](https://github.com/boykopovar/AnyPS5).

Ovaj podsustav omogućuje developerima i maintainerima da sigurno „posude” snagu svojih računala (CPU/GPU) kako bi se riješio problem preskočenih C++ buildova (`Build / windows`, `Build / linux`, `relinker`, `register-context`, `write-watch`) na GitHub Actions.

---

## 🚀 Brzi početak: Kako postati AnyPS5 Runner

### Opcija 1: Windows (Izravno preko PowerShell-a)

1. Otvorite PowerShell u mapi projekta.
2. Pokrenite pripremljenu skriptu:
   ```powershell
   .\runner\setup_windows_runner.ps1
   ```
   *Skripta će automatski provjeriti prisutnost `git`, `cmake`, `ninja`, `clang` ili `MSVC` alata, preuzeti službeni GitHub Actions Runner za Windows x64 i zatražiti registracijski token.*
3. Ako imate GitHub CLI (`gh`), token se može automatski generirati ili unesite token koji vam je dao maintainer.
4. Vaš PC je spojen i čeka C++ build poslove! Pritisnite `Ctrl+C` bilo kada da zaustavite runner.

---

### Opcija 2: Linux / Docker (100% Izolirano u kontejneru)

Za Linux korisnike ili one koji žele potpunu izolaciju bez instalacije C++ alata na host sustav:

```bash
cd runner
export RUNNER_TOKEN="VAŠ_GITHUB_TOKEN"
docker compose up -d --build
```

Kontejner dolazi s preinstaliranim `clang-18`, `llvm-18`, `cmake`, `ninja` i Vulkan bibliotekama.

---

## 🛡️ Sigurnost (Security Invariants)

Zaštita vašeg računala je na prvom mjestu:

1. **Approval Gating:** Poslovi se **nikada** ne pokreću automatski s nepoznatih PR-ova. Pokreću se samo ako:
   - PR dolazi od službenog člana/maintainera (`MEMBER`, `COLLABORATOR`, `OWNER`).
   - Ili je maintainer pregledao kod i dodao GitHub labelu `ci:run-build` ili `safe-to-test`.
2. **Ephemeral način rada:** Moguće je pokrenuti runner s parametrom `-Ephemeral` (runner odradi 1 build posao i odmah se sam odjavi).

---

## 🛠️ Upravljačka skripta (za Maintainere)

Dostupna je CLI skripta `runner/anyps5_runner_manager.py`:

```bash
# 1. Prikaz svih registriranih runnera i njihov online status:
python runner/anyps5_runner_manager.py --status

# 2. Generiranje svježeg registracijskog tokena za novog developera:
python runner/anyps5_runner_manager.py --get-token

# 3. Pokretanje testnog builda za određeni PR broj na community runnerima:
python runner/anyps5_runner_manager.py --trigger-pr 6834
```

---

## 📦 Struktura datoteka

| Datoteka | Opis |
| :--- | :--- |
| `runner/setup_windows_runner.ps1` | Automatizirana PowerShell skripta za postavljanje na Windowsima |
| `runner/Dockerfile` | Docker slika s Ubuntu 24.04 i Clang-18 okruženjem |
| `runner/docker-compose.yml` | Jednostavno pokretanje Linux runnera u kontejneru |
| `runner/entrypoint.sh` | Životni ciklus i automatsko brisanje runnera |
| `runner/anyps5_runner_manager.py` | Python alat za pregled i slanje buildova |
| `.github/workflows/anyps5_self_hosted_ci.yml` | GitHub Actions workflow s approval gatingom |
