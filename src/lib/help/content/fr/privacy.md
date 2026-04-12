# Aperçu de la confidentialité
{app} est conçu pour fonctionner entièrement en local. Vos données EEG, embeddings, labels et paramètres ne quittent jamais votre machine.

# Stockage des données

## Toutes les données restent sur votre appareil
Toutes les données enregistrées par {app} — échantillons EEG bruts (CSV), embeddings EEG (SQLite + index HNSW), labels textuels, horodatages de calibration, journaux et paramètres — sont stockées localement dans {dataDir}/. Aucune donnée n'est envoyée vers un service cloud, un serveur ou un tiers.

## Aucun compte utilisateur
{app} ne nécessite aucune inscription, connexion ou création de compte.

## Emplacement des données
Tous les fichiers sont sous {dataDir}/ sur macOS et Linux. Chaque jour a son propre sous-répertoire AAAAMMJJ.

# Activité réseau

## Aucune télémétrie ni analytique
{app} ne collecte aucune analytique d'utilisation, rapport de crash ou suivi comportemental.

## Serveur WebSocket local uniquement
{app} exécute un serveur WebSocket lié à votre interface réseau locale pour le streaming LAN.

## Service mDNS / Bonjour
{app} enregistre un service mDNS _skill._tcp.local. pour la découverte automatique sur le réseau local.

## Vérification des mises à jour
Lorsque vous cliquez sur « Vérifier les mises à jour », {app} contacte le point de terminaison configuré. C'est la seule requête internet.

# Bluetooth & Sécurité

## Bluetooth Low Energy (BLE)
{app} communique avec votre appareil BCI via Bluetooth Low Energy ou liaison USB série. La connexion utilise la pile système standard.

## Permissions système
L'accès Bluetooth nécessite une permission système explicite.

## Identifiants d'appareil
Le numéro de série et l'adresse MAC du casque BCI sont stockés uniquement localement.

# Traitement sur l'appareil

## Inférence GPU locale
L'encodeur d'embedding EEG fonctionne entièrement sur votre GPU locale via wgpu. Les poids du modèle sont chargés depuis le cache local Hugging Face (~/.cache/huggingface/). Aucune donnée EEG n'est envoyée à une API d'inférence externe ou un GPU cloud. Les embeddings textuels pour la recherche de labels utilisent nomic-embed-text-v1.5, également mis en cache localement.

## Filtrage et analyse
Tout le traitement du signal s'exécute localement sur votre CPU/GPU.

## Recherche par plus proches voisins
L'index vectoriel HNSW est construit et interrogé entièrement sur votre appareil.

# Vos données, votre contrôle

## Accès
Toutes vos données sont dans {dataDir}/ en formats standards (CSV, SQLite, HNSW binaire).

## Suppression
Supprimez n'importe quel fichier sous {dataDir}/ à tout moment. Pas de sauvegardes cloud.

## Exportation
Les enregistrements CSV et bases SQLite sont des formats portables standards.

## Chiffrement
{app} ne chiffre pas les données au repos. Utilisez le chiffrement de disque de votre système d'exploitation.

# Suivi d'activité

## Suivi d'activité
Lorsqu'il est activé, NeuroSkill enregistre quelle application est au premier plan et quand le clavier et la souris ont été utilisés en dernier. Ces données restent entièrement sur votre appareil dans ~/.skill/activity.sqlite - elles ne sont jamais envoyées à un serveur, journalisées à distance ou incluses dans des analyses. Le suivi de fenêtre active capture : nom de l'application, chemin de l'exécutable, titre de la fenêtre et horodatage Unix. Le suivi clavier/souris capture seulement deux horodatages - jamais les frappes, le texte tapé, les coordonnées du curseur ni les cibles de clic. Les deux fonctions peuvent être désactivées indépendamment dans Paramètres → Suivi d'activité.

## Permission Accessibilité (macOS)
Sur macOS, le suivi clavier/souris nécessite la permission Accessibilité car il installe un CGEventTap. Apple exige cette permission pour toute application lisant les entrées globales. Sans elle, le hook échoue silencieusement : l'app continue de fonctionner normalement, seuls les horodatages d'entrée restent à zéro. Le suivi de fenêtre active utilise osascript et ne nécessite pas la permission Accessibilité.

# Résumé

## No cloud
Pas de cloud. Toutes les données sont stockées localement dans {dataDir}/.

## No telemetry
Pas de télémétrie. Aucune analytique ni suivi.

## No accounts
Pas de compte. Aucune inscription ni identifiant.

## One optional network request
Une seule requête réseau optionnelle. Vérification de mises à jour uniquement.

## Fully on-device
Entièrement sur l'appareil. Inférence GPU, traitement du signal et recherche en local.

## Activity tracking is local-only
Suivi d'activité local uniquement. Le focus des fenêtres et les horodatages d'entrée sont écrits dans activity.sqlite sur votre appareil et n'en sortent jamais.
