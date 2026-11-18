# ExploRust

Un explorateur de fichiers pour macOS, écrit en Rust avec [GPUI](https://www.gpui.rs). Léger, rapide, épuré.

## Fonctionnalités

- Sidebar (Home, Desktop, Documents, Downloads, Applications)
- Navigation précédent / suivant / dossier parent
- Liste Nom / Date de modification / Taille, dossiers en premier
- Barre de recherche : filtre le dossier courant pendant la frappe
- Tri par colonne : clic sur l'en-tête pour trier, nouveau clic pour inverser
- Chemin cliquable et nombre d'éléments en bas
- Mises à jour intégrées (fenêtre de proposition, installation en un clic)
- Thème clair ou sombre automatique, et couleur d'accentuation du système (suivis en direct)
- Ouverture des fichiers avec l'app par défaut

## Installer l'app (DMG)

1. Télécharge `ExploRust-x.y.z.dmg` depuis la page [Releases](https://github.com/erdemm1020/ExploRust/releases).
2. Ouvre-le et glisse **ExploRust** dans **Applications**.
3. Au premier lancement, macOS peut refuser d'ouvrir l'app (elle n'est pas notarisée par Apple) : fais un clic droit sur l'app, **Ouvrir**, puis confirme. Ou, dans un terminal :

```sh
xattr -dr com.apple.quarantine /Applications/ExploRust.app
```

L'app est universelle (Apple Silicon et Intel) et demande macOS 12 ou plus.

## Mises à jour

ExploRust regarde au démarrage s'il existe une release plus récente. Si oui, une fenêtre propose :

- **Install and Restart** : télécharge le DMG, vérifie sa somme de contrôle (SHA-256) et la signature de l'app, remplace l'app installée, puis la relance ;
- **Later** : redemandera au prochain lancement ;
- **Skip This Version** : ne redemande plus pour cette version.

Si l'installation échoue (dossier non modifiable, réseau coupé…), l'ancienne version reste intacte et la fenêtre propose d'ouvrir la page de téléchargement.

Dans **Settings**, la section **Updates** permet de désactiver la vérification automatique ou de lancer **Check for Updates** à la main. La vérification interroge uniquement l'API publique de GitHub, sans rien envoyer sur toi. **Le dépôt doit être public** pour que les releases soient visibles par l'app.

Les mises à jour ne fonctionnent que depuis l'app installée (`ExploRust.app`), pas depuis `cargo run`.

## Publier une release

```sh
scripts/release.sh 0.2.0          # change la version, commit, crée le tag v0.2.0
git push && git push origin v0.2.0
```

Le tag lance le workflow GitHub Actions (`.github/workflows/release.yml`) : il compile l'app universelle, crée `ExploRust-0.2.0.dmg` et son fichier `.sha256`, et publie la release. Les copies d'ExploRust déjà installées proposent alors la mise à jour.

Pour fabriquer l'app et le DMG en local :

```sh
scripts/bundle.sh      # dist/ExploRust.app
scripts/make-dmg.sh    # dist/ExploRust-<version>.dmg
```

## Compiler depuis les sources

### Prérequis

- macOS
- [Rust](https://rustup.rs) (édition 2024, donc Rust 1.85 ou plus récent)
- Xcode (l'app complète, pas seulement les Command Line Tools) : GPUI utilise Metal

Installer Rust :

```sh
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```

Accepter la licence Xcode et pointer vers l'app :

```sh
sudo xcodebuild -license accept
sudo xcode-select -s /Applications/Xcode.app/Contents/Developer
```

### Installation

```sh
git clone <url-du-repo> ExploRust
cd ExploRust
```

### Lancement

Mode développement (compilation rapide) :

```sh
cargo run
```

Mode release (optimisé, recommandé pour l'usage) :

```sh
cargo run --release
```

On peut aussi ouvrir un dossier précis : `cargo run --release -- ~/Documents`.

Le binaire optimisé se trouve dans `target/release/ExploRust`. Pour l'installer dans `~/.cargo/bin` :

```sh
cargo install --path .
ExploRust
```

La première compilation télécharge et compile GPUI : compte quelques minutes.

## Raccourcis

| Raccourci | Action |
|---|---|
| Double-clic, `⌘O` | Ouvrir la sélection (un dossier : y entrer ; fichiers : les ouvrir tous) |
| `Enter` | Renommer l'élément sélectionné (`Enter` valide, `Échap` annule) |
| `↑` / `↓` | Changer la sélection |
| `⇧↑` / `⇧↓` | Étendre la sélection |
| `⇧` + clic | Sélectionner une plage |
| `⌘` + clic | Ajouter / retirer un élément de la sélection |
| `⌘⇧N` | Nouveau dossier (renommé aussitôt) |
| `⌘⌫` | Mettre la sélection à la corbeille (confirmation si un dossier contient plus de 100 éléments) |
| `⌘F` | Rechercher dans le dossier (`↓` passe à la liste, `Échap` efface) |
| `⌘,` | Ouvrir / fermer les réglages (`Échap` ferme) |
| `⌘A` | Tout sélectionner |
| `⌘C` / `⌘V` | Copier / coller des fichiers (compatible avec le vrai Finder et les autres apps) |
| `⌘⌥V` | Déplacer ici les fichiers copiés (au lieu de les copier) |
| `⌘⌥C` | Copier le chemin du fichier (texte) |
| `Échap` | Désélectionner |
| `⌘↑` | Dossier parent |
| `⌘↓` | Ouvrir la sélection |
| `⌘[` / `⌘]` | Précédent / suivant |
| Boutons latéraux de la souris | Précédent / suivant |
| `⌘⇧.` | Afficher / masquer les fichiers cachés |
| Glisser-déposer | Déplacer vers un dossier, la sidebar ou le chemin du bas |
| `⌥` + glisser-déposer | Copier au lieu de déplacer |
| Déposer depuis une autre app | Copier dans le dossier visé ou le dossier courant |
| Clic sur un en-tête de colonne | Trier par Name, Date Modified ou Size (un 2e clic inverse l'ordre) |
| Clic droit sur un élément | Ouvrir, révéler dans le Finder, Copy / Paste Item, Open With (choisir une app, ou la définir par défaut), ouvrir un dossier dans le terminal, copier le chemin, renommer, mettre à la corbeille |
| Clic droit dans le vide | Nouveau dossier ou nouveau fichier (renommés aussitôt), ouvrir le terminal ici (ou « Open in… » pour en choisir un autre), afficher / masquer les fichiers cachés |
| `⌘Q` | Quitter |

## Réglages

Clique sur **Settings** en bas à gauche de la sidebar (ou `⌘,`). Les changements s'appliquent tout de suite et sont enregistrés.

| Section | Option |
|---|---|
| Appearance | Thème (système, clair, sombre), couleur d'accent, hauteur des lignes |
| Files | Tri naturel (`file2` avant `file10`), dossiers en haut, fichiers cachés, dates relatives |
| Updates | Vérification automatique des mises à jour, vérification manuelle |
| Behavior | Ouvrir un dossier d'un clic, rafraîchissement automatique, rouvrir le dernier dossier, seuil d'avertissement avant la corbeille, terminal |

« Reset to Defaults » remet tout à zéro. Le fichier `~/Library/Application Support/ExploRust/config` peut aussi s'éditer à la main.

## Terminal par défaut

Par défaut, « Open in Terminal » utilise l'app qui ouvre les fichiers `.command` (Terminal.app sauf réglage contraire).
Pour en changer : choisis-le dans les réglages, ou clic droit dans un dossier, **Open in…**, puis survole un terminal et clique **Set as Default**
(Terminal, iTerm, Ghostty, Warp, Alacritty, kitty, WezTerm… sont détectés), ou **Choose Application…** pour une autre app.

Le choix est gardé dans `~/Library/Application Support/ExploRust/config` :

```
terminal = /Applications/Ghostty.app
```

Supprime ce fichier pour revenir au terminal du système.

## Problèmes courants

**`cannot execute tool 'metal' due to missing Metal Toolchain`**

Le compilateur Metal n'est pas installé. Deux solutions :

- Installer le composant : `xcodebuild -downloadComponent MetalToolchain`
- Ou, comme dans ce projet, utiliser la feature GPUI `runtime_shaders` (déjà activée dans `Cargo.toml`) : les shaders sont compilés au lancement, sans Metal Toolchain. Le démarrage est un peu plus lent.

**`xcode-select: error: tool 'xcodebuild' requires Xcode`**

Seules les Command Line Tools sont installées. Installe Xcode depuis l'App Store, puis relance `xcode-select -s` comme plus haut.
