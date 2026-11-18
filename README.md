# ExploRust

Un explorateur de fichiers pour macOS, écrit en Rust avec [GPUI](https://www.gpui.rs). Léger, rapide, épuré.

## Fonctionnalités

- Sidebar (Home, Desktop, Documents, Downloads, Applications)
- Navigation précédent / suivant / dossier parent
- Liste Nom / Taille, dossiers en premier
- Chemin cliquable et nombre d'éléments en bas
- Thème clair ou sombre automatique
- Ouverture des fichiers avec l'app par défaut

## Prérequis

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

## Installation

```sh
git clone <url-du-repo> ExploRust
cd ExploRust
```

## Lancement

Mode développement (compilation rapide) :

```sh
cargo run
```

Mode release (optimisé, recommandé pour l'usage) :

```sh
cargo run --release
```

Le binaire optimisé se trouve dans `target/release/ExploRust`. Pour l'installer dans `~/.cargo/bin` :

```sh
cargo install --path .
ExploRust
```

La première compilation télécharge et compile GPUI : compte quelques minutes.

## Raccourcis

| Raccourci | Action |
|---|---|
| Double-clic, `Enter` | Ouvrir le dossier ou fichier sélectionné |
| `↑` / `↓` | Changer la sélection |
| `⌘↑` | Dossier parent |
| `⌘↓` | Ouvrir la sélection |
| `⌘[` / `⌘]` | Précédent / suivant |
| `⌘⇧.` | Afficher / masquer les fichiers cachés |
| `⌘Q` | Quitter |

## Problèmes courants

**`cannot execute tool 'metal' due to missing Metal Toolchain`**

Le compilateur Metal n'est pas installé. Deux solutions :

- Installer le composant : `xcodebuild -downloadComponent MetalToolchain`
- Ou, comme dans ce projet, utiliser la feature GPUI `runtime_shaders` (déjà activée dans `Cargo.toml`) : les shaders sont compilés au lancement, sans Metal Toolchain. Le démarrage est un peu plus lent.

**`xcode-select: error: tool 'xcodebuild' requires Xcode`**

Seules les Command Line Tools sont installées. Installe Xcode depuis l'App Store, puis relance `xcode-select -s` comme plus haut.
