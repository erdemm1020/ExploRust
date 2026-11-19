# ExploRust

Un explorateur de fichiers pour macOS, écrit en Rust avec [GPUI](https://www.gpui.rs). Léger, rapide, épuré.

## Fonctionnalités

- Sidebar redimensionnable (glisse son bord), repliable en simple rangée d'icônes (double-clic sur le bord ou `⌘⌥S`)
- Icônes par type de fichier : simples ou avancées (PDF rouge, tableur vert, code coloré par langage, dossiers Téléchargements / Images / Musique…)
- Navigation précédent / suivant / dossier parent
- Liste Nom / Date de modification / Taille, dossiers en premier
- Barre de recherche : filtre le dossier courant pendant la frappe
- Tri par colonne : clic sur l'en-tête pour trier, nouveau clic pour inverser
- Chemin cliquable et nombre d'éléments en bas
- Annuler (`⌘Z`), copies et déplacements en arrière-plan avec barre de progression, zip et dézip
- Taille des dossiers affichée, panneau Infos (`⌘I`)
- Pour les développeurs (désactivable en un clic) : Git, reconnaissance de projets, serveurs SSH, Claude
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
| `⌘Z` / `⌘⇧Z` | Annuler / rétablir (renommer, déplacer, copier, corbeille, créer, zip) |
| `⌘I` | Infos : dates, permissions, propriétaire, taille réelle d'un dossier |
| `⌘K` | Se connecter à un serveur (SSH) |
| `⌘⌥S` | Replier / ouvrir la sidebar |
| `⌘A` | Tout sélectionner |
| `⌘C` / `⌘V` | Copier / coller des fichiers (compatible avec le vrai Finder et les autres apps) |
| `⌘⌥V` | Déplacer ici les fichiers copiés (au lieu de les copier) |
| `⌘⌥C` | Copier le chemin du fichier (texte) |
| `Échap` | Désélectionner |
| `⌘↑` | Dossier parent |
| `⌘↓` | Ouvrir la sélection |
| `⌘[` / `⌘]` | Précédent / suivant |
| Boutons latéraux de la souris | Précédent / suivant |
| `Espace` | Quick Look : aperçu de la sélection, `Espace` à nouveau pour fermer |
| `⌘⇧.` | Afficher / masquer les fichiers cachés (affichés tout en haut de la liste) |
| Glisser-déposer | Déplacer vers un dossier, la sidebar ou le chemin du bas |
| `⌥` + glisser-déposer | Copier au lieu de déplacer |
| Glisser hors de la fenêtre | Déposer dans le Finder, une autre app ou une autre fenêtre (copie) |
| Déposer depuis une autre app | Copier dans le dossier visé ou le dossier courant |
| Clic sur un en-tête de colonne | Trier par Name, Date Modified ou Size (un 2e clic inverse l'ordre) |
| Clic droit sur un élément | Ouvrir, révéler dans le Finder, Copy / Paste Item, Open With (choisir une app, ou la définir par défaut), ouvrir dans VS Code / Zed / un terminal, copier le chemin, renommer, mettre à la corbeille |
| Clic droit dans le vide | Nouveau dossier ou nouveau fichier (renommés aussitôt), ouvrir le terminal ici (ou « Open in… » pour en choisir un autre), afficher / masquer les fichiers cachés |
| `⌘Q` | Quitter |

## Réglages

Clique sur **Settings** en bas à gauche de la sidebar (ou `⌘,`). Les changements s'appliquent tout de suite et sont enregistrés.

| Section | Option |
|---|---|
| Appearance | Thème (système, clair, sombre), couleur d'accent, icônes (simples ou avancées), hauteur des lignes |
| Files | Tri naturel (`file2` avant `file10`), dossiers en haut, fichiers cachés, tailles de dossiers, dates relatives |
| Developer | Interrupteur général des outils développeur, Git, fichiers ignorés en grisé, projets, serveurs SSH, Claude (modèle, clé) |
| Updates | Vérification automatique des mises à jour, vérification manuelle |
| Behavior | Ouvrir un dossier d'un clic, rafraîchissement automatique, rouvrir le dernier dossier, seuil d'avertissement avant la corbeille, éditeur et terminal par défaut |

« Reset to Defaults » remet tout à zéro. Le fichier `~/Library/Application Support/ExploRust/config` peut aussi s'éditer à la main.

## Menu clic droit

« Share with AirDrop… » envoie les fichiers sélectionnés vers un autre appareil Apple (affiché seulement si ce Mac peut faire de l'AirDrop).

Le menu s'adapte à ses libellés (jamais coupés), et se pilote aussi au clavier : `↑` `↓` pour choisir, `Entrée` pour valider, `→` / `←` pour ouvrir ou fermer un sous-menu, `Échap` pour fermer.

## Annuler, progression, archives, infos

- **Annuler** : `⌘Z` défait le dernier renommage, déplacement, copie, création, compression ou mise à la corbeille (50 opérations gardées), `⌘⇧Z` le rétablit. Un message en bas confirme.
- **Copier et déplacer en arrière-plan** : la fenêtre reste utilisable, une barre de progression s'affiche en bas avec un bouton ✕ pour annuler (ce qui a déjà été copié est retiré).
- **Zip** : clic droit, **Compress** (un élément : `nom.zip`, plusieurs : `Archive.zip`) ou **Extract** sur un `.zip`, `.tar`, `.tar.gz`, `.tgz`, `.tar.bz2` ou `.tar.xz`. Les noms ne sont jamais écrasés.
- **Tailles de dossiers** : la colonne Size les affiche (calculées en arrière-plan, un dossier à la fois, 4 s maximum chacun). Réglage : *Folder sizes*.
- **Infos** (`⌘I`, ou clic droit, *Get Info*) : type, taille, dates, permissions, propriétaire, lien, et un bouton pour rendre un fichier exécutable.

## Outils pour développeurs

Réglages, section **Developer** : un interrupteur général *Developer tools* cache tout ce qui suit si tu ne codes pas, et chaque fonction se coupe seule.

- **Git** : lettre de statut à droite du nom (`M` modifié, `A` ajouté, `?` nouveau, `D` supprimé, `R` renommé, `!` conflit, `•` un dossier qui contient des changements), fichiers ignorés par `.gitignore` en grisé, et en bas la branche (`⎇ main ↑2 ↓1 · 3 changes`). Clic droit : *Git: Stage*, *Unstage*, *Discard Changes…* (avec confirmation) et *Open on the Web* (GitHub ou GitLab). Les lectures n'utilisent pas de verrou : elles ne gênent jamais un `git` lancé en parallèle.
- **Projets** : un petit tag (*Rust*, *Node*, *Go*, *Python*, *Swift*, *Java*, *Ruby*, *PHP*, *Make*) sur les dossiers reconnus. Clic droit sur un projet (ou dans le dossier), les commandes courantes (`cargo run`, `npm run dev`, `go test ./...`…) s'ouvrent dans ton terminal.
- **Serveurs SSH** : `⌘K` (ou *Connect to Server…* dans la sidebar, ou `ExploRust ssh://hôte/dossier`). Les alias de `~/.ssh/config` apparaissent dans la sidebar. Un dossier distant s'affiche comme un dossier local : ouvrir (télécharge et ouvre), *Download*, renommer, nouveau dossier, supprimer (toujours avec confirmation, sans corbeille), *Open a shell in <terminal>*, et glisser des fichiers dessus pour les envoyer. Utilise `ssh` et `scp` du Mac avec tes clés (pas de mot de passe) ; un serveur inconnu est accepté à la première connexion.
- **Claude** (désactivé par défaut) : *Explain This*, *Suggest a Name*, *Summarize This Folder* et *Suggest an Organization* dans le clic droit. Active-le dans les réglages, puis copie ta clé d'API (`sk-ant-…`) et clique *Save from Clipboard* : elle est rangée dans le Trousseau macOS, jamais dans un fichier. **Ce qui est envoyé à Anthropic** : uniquement le début (20 ko) du fichier choisi, ou la liste des noms d'un dossier (200 maximum) et le début de son README, et seulement quand tu cliques. Les fichiers qui ressemblent à des secrets (`.env`, clés SSH, `.aws`, trousseaux…) ne sont jamais envoyés. Modèle au choix (Opus 5.5 par défaut, Sonnet 5.5, Haiku 5.5).

## Ouvrir dans un éditeur ou un terminal

Clic droit sur un élément (ou dans le vide) :

- **Open in <éditeur>** ouvre le dossier ou les fichiers dans ton éditeur (VS Code, Zed, Cursor, Windsurf, VSCodium, Sublime Text, Nova…). Par défaut, le premier trouvé sur ton Mac.
- **Open in <terminal>** ouvre le dossier dans ton terminal.
- **Open in…** liste tous les éditeurs et terminaux installés : un clic ouvre une fois avec celui-ci, **Set as Default** (au survol) le choisit comme défaut, **Choose Editor… / Choose Terminal…** ouvre le sélecteur du Mac pour une autre app.

Le choix se règle aussi dans **Settings → Behavior → Editor / Terminal**.

## Terminal par défaut

Par défaut, « Open in Terminal » utilise l'app qui ouvre les fichiers `.command` (Terminal.app sauf réglage contraire).
Pour en changer : choisis-le dans les réglages, ou clic droit, **Open in…** puis **Set as Default**, puis survole un terminal et clique **Set as Default**
(Terminal, iTerm, Ghostty, Warp, Alacritty, kitty, WezTerm… sont détectés), ou **Choose Application…** pour une autre app.

Le choix est gardé dans `~/Library/Application Support/ExploRust/config` :

```
terminal = /Applications/Ghostty.app
editor = /Applications/Zed.app
```

Supprime ce fichier pour revenir au terminal du système.

## Problèmes courants

**`cannot execute tool 'metal' due to missing Metal Toolchain`**

Le compilateur Metal n'est pas installé. Deux solutions :

- Installer le composant : `xcodebuild -downloadComponent MetalToolchain`
- Ou, comme dans ce projet, utiliser la feature GPUI `runtime_shaders` (déjà activée dans `Cargo.toml`) : les shaders sont compilés au lancement, sans Metal Toolchain. Le démarrage est un peu plus lent.

**`xcode-select: error: tool 'xcodebuild' requires Xcode`**

Seules les Command Line Tools sont installées. Installe Xcode depuis l'App Store, puis relance `xcode-select -s` comme plus haut.
