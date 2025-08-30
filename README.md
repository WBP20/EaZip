# EaZip

Application de bureau multiplateforme pour chiffrer et déchiffrer des fichiers ZIP avec différentes méthodes de chiffrement, construite avec Tauri, React et TypeScript.

## Fonctionnalités

*   Chiffrement de fichiers et dossiers en archives ZIP.
*   Prise en charge de plusieurs méthodes de chiffrement (AES-256, CryptoZip, 7-Zip).
*   Génération de mots de passe sécurisés.
*   Interface utilisateur intuitive avec glisser-déposer.
*   Affichage de la progression du chiffrement.
*   Compatible avec Windows, macOS et Linux.

## Technologies Utilisées

*   **Tauri** : Framework pour construire des applications de bureau multiplateformes avec des technologies web.
*   **React** : Bibliothèque JavaScript pour construire l'interface utilisateur.
*   **TypeScript** : Langage de programmation qui ajoute le typage statique à JavaScript.
*   **Tailwind CSS** : Framework CSS utilitaire pour un stylisme rapide et personnalisable.
*   **Rust** : Langage de programmation utilisé pour le backend de Tauri.

## Prérequis

Assurez-vous d'avoir les éléments suivants installés sur votre machine :

*   [Node.js](https://nodejs.org/) (version 18 ou supérieure recommandée)
*   [npm](https://www.npmjs.com/) (généralement inclus avec Node.js) ou [Yarn](https://yarnpkg.com/)
*   [Rust](https://www.rust-lang.org/tools/install) (avec `rustup`)
*   [Tauri CLI](https://tauri.app/v1/guides/getting-started/prerequisites#install-tauri-cli) (`cargo install tauri-cli`)

## Installation

```bash
# Clonez le dépôt
git clone https://github.com/votre-utilisateur/EaZip.git
cd EaZip/anyzip

# Installez les dépendances frontend
npm install # ou yarn install

# Installez les dépendances Rust (si ce n'est pas déjà fait)
# cargo install tauri-cli
```

## Utilisation

### Mode développement

Pour lancer l'application en mode développement (avec rechargement à chaud) :

```bash
npm run tauri dev
```

### Construction de l'application (Build)

Pour construire l'application pour votre système d'exploitation :

```bash
npm run tauri build
```

Les exécutables seront générés dans le dossier `anyzip/src-tauri/target/release/bundle/`.

## Licence
MIT
