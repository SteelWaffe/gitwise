# Gitwise

Hesaplarını birbirinden ayıran, ne yaptığını gösteren hafif bir git istemcisi. Tauri 2 + React + Rust.

Şirket ve kişisel hesapla çalışırken git iki şeyi birbirinden bağımsız tutar:

| | Nereden gelir | Karışınca ne olur |
|---|---|---|
| **Commit kimliği** (isim + e-posta) | `user.name` / `user.email`, çoğunlukla global ayar | Şirket reposuna kişisel e-postayla commit gider |
| **Push yetkisi** (token / SSH anahtarı) | Git Credential Manager, `github.com` başına çoğunlukla tek hesap | Yanlış hesapla push, "permission denied" |

Gitwise ikisini **profil** altında birleştirir ve her repoyu bir profile bağlar.

## Nasıl çalışır

- **Profil:** etiket, renk, servis (GitHub, GitHub Enterprise, GitLab, Bitbucket, Azure DevOps, genel), commit kimliği, token ve isteğe bağlı SSH anahtarı. Token'lar ve SSH parolaları uygulamanın JSON dosyasında değil, işletim sisteminin kimlik deposunda durur (Windows Kimlik Bilgileri Yöneticisi, macOS Anahtar Zinciri).
- **Commit:** `git -c user.name=… -c user.email=… commit`. Kimlik her seferinde profilden gelir, global ayara bakılmaz.
- **Push/pull/fetch/clone:**
  - `-c credential.helper=` her yerde tanımlı kimlik yardımcılarını (GCM dahil) bu komut için devre dışı bırakır.
  - `GIT_ASKPASS` ve `SSH_ASKPASS` Gitwise'ın kendi çalıştırılabilir dosyasını gösterir.
  - Git kimlik sorduğunda Gitwise "askpass modunda" açılır, token'ı kimlik deposundan okuyup yazar ve kapanır.
  - Yalnızca profilin sunucusuna cevap verir; başka bir sunucudaki submodule ya da LFS adresine token gitmez.
  - SSH için `GIT_SSH_COMMAND="ssh -i <anahtar> -o IdentitiesOnly=yes"` kullanılır; başka anahtar denenmez.
- **Repo'ya bağlama** reponun kendi `.git/config` dosyasına şunları yazar:
  - `user.name`, `user.email`
  - `credential.https://host.username`
  - gerekirse `core.sshCommand`
  - `gitwise.profile`

  Böylece aynı klasörde terminal, VS Code ve Rider da doğru kimlikle çalışır.
- **Kurallar** yeni eklenen ya da klonlanan repoların profilini otomatik seçer, ilk eşleşen kural kazanır. Örnekler: `github.com/sirketin` → İş, `D:\Kisisel` → Kişisel.
- **Koruma (guard)** her commit ve push'tan önce şunları kontrol eder:
  - profil seçilmemiş
  - kural başka bir profili gösteriyor
  - profil başka bir sunucu için
  - repo geçmişi başka bir profilden
  - gönderilecek commit'lerde başka e-postalar var
  - token eksik
  - LFS hazır değil

  Her uyarının yanında tek tıkla çözüm vardır. **Yanlış kimlikle atılmış ama henüz gönderilmemiş commit'ler** tek tuşla doğru profile çevrilir. Gönderilmiş geçmişe dokunulmaz.
- **Şeffaflık:** çalışan her git komutu "Etkinlik" panelinde kopyalanabilir haliyle, hangi profille çalıştığı ve çıktısıyla görünür.
- **Git'in kendisi:** Gitwise bilgisayardaki gerçek `git`'i çalıştırır. LFS, hook'lar (husky vb.), sparse checkout ve partial clone terminalde nasıl çalışıyorsa burada da öyle çalışır.

## Proje yapısı

```
crates/gitwise-core/     Motor: git çalıştırıcı, profiller, kurallar, koruma, askpass, sağlayıcı API'leri
  tests/real_git.rs      Gerçek git ile uçtan uca testler
apps/desktop/            Tauri uygulaması
  src-tauri/             Rust tarafı (komutlar, askpass giriş noktası)
  src/                   React arayüzü (Türkçe/İngilizce)
  src/mock.ts            Tarayıcıda Tauri olmadan arayüz geliştirmek için sahte backend
scripts/set-version.mjs  CI'da sürümü ve güncelleme adresini damgalar
.github/workflows/       ci.yml (her push/PR), release.yml (main'e her merge)
```

## Geliştirme

Gerekenler: Node 22+, Rust (stable), Git.

```bash
cd apps/desktop
npm install
npm run tauri dev
```

Yalnızca arayüz üzerinde çalışırken Tauri olmadan, sahte verilerle açmak için `npm run dev` (http://localhost:1430) yeterli.

Testler:

```bash
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
```

## Sürüm ve otomatik güncelleme

`main`'e her merge'de `release.yml` çalışır:

1. Testler ve lint çalışır (`ci.yml`).
2. Sürüm `VERSION` (major.minor) ve çalıştırma numarasından oluşur, örneğin `0.1.37`. Büyük bir sürüm atlamak için `VERSION` dosyasını değiştir.
3. Windows kurulum dosyası (NSIS, kullanıcı başına kurulum, yönetici izni istemez) derlenir ve güncelleme imzasıyla imzalanır.
4. GitHub Release yayınlanır ve `latest.json` eklenir.
5. Çalışan Gitwise kopyaları açılışta ve 6 saatte bir `releases/latest/download/latest.json` adresini kontrol eder. Yeni sürüm varsa "Güncelle ve yeniden başlat" çıkar.

Bir kerelik kurulum (GitHub repo ayarları):

| Ad | Tür | Değer |
|---|---|---|
| `TAURI_SIGNING_PRIVATE_KEY` | Secret | `~/.tauri/gitwise.key` dosyasının içeriği (repoda **yok**, kaybedilirse eski kurulumlar güncellenemez) |
| `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` | Secret | Boş (anahtar parolasız üretildi) |
| `GITWISE_GITHUB_CLIENT_ID` | Variable (isteğe bağlı) | "Tarayıcıyla giriş" için GitHub OAuth App Client ID'si |

Notlar:
- **Güncelleme akışı herkese açık bir release adresi ister.** Repo özelse, release'lerin herkese açık bir repoda yayınlanması gerekir. Diğer seçenek, güncelleme adresini başka bir yere (ör. kendi sunucun) yönlendirmektir.
- **Windows kod imzalama sertifikası yok:** kurulumda SmartScreen "bilinmeyen yayıncı" uyarısı çıkar. Güncellemeler bundan etkilenmez, çünkü onlar Tauri'nin kendi imzasıyla doğrulanır.
- **GitHub OAuth App:**
  1. GitHub > Settings > Developer settings > OAuth Apps > New.
  2. "Enable Device Flow" kutusunu işaretle.
  3. Callback URL olarak herhangi bir adres yazılabilir.

  Client ID'yi değişkene ya da uygulamada Ayarlar > Genel'e gir. Organizasyonlar üçüncü taraf OAuth uygulamalarını kısıtlıyorsa, organizasyon yöneticisinin uygulamayı onaylaması gerekir. Token ile giriş her durumda çalışır.

## macOS

Kod platformdan bağımsız yazıldı:
- kimlik deposu macOS'ta Keychain
- veri klasörü `~/Library/Application Support/Gitwise`
- Homebrew yolları PATH'e ekleniyor
- macOS paket ayarları `tauri.macos.conf.json` içinde

`ci.yml` her push'ta macOS'ta da derleyip test eder. Yayınlamak için `release.yml` içindeki macOS satırını aç. Notarize edilmiş bir uygulama için Apple imzalama secret'larını da ekle.

## Yol haritası

- **v0.1 (bu sürüm):**
  - profiller, kurallar, koruma, yazar düzeltme
  - değişiklikler, diff, commit, geçmiş, branch'ler
  - fetch/pull/push, clone/init
  - LFS algılama ve kurulumu, etkinlik paneli
  - otomatik güncelleme
- **v0.2:** çakışma çözme ekranı, stash, satır/parça bazında commit, commit düzenleme (amend), uzak repo ekleme ve yayınlama.
- **v0.3:** LFS dosya kilitleme (Unreal ekipleri için), sparse checkout ve partial clone arayüzü, büyük repo hızlandırmaları (fsmonitor).
- **Sonrası:** pull request görünümü, interaktif rebase, commit imzalama (SSH/GPG).
