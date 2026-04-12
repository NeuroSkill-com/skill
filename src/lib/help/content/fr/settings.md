# Onglet Paramètres
Configurez les préférences d'appareil, le traitement du signal, les paramètres d'embedding, la calibration, les raccourcis et la journalisation.

## Appareils appariés
Liste tous les appareils BCI détectés. Vous pouvez définir un appareil préféré, oublier des appareils ou en rechercher de nouveaux.

## Traitement du signal
Configurez la chaîne de filtrage EEG en temps réel : passe-bas, passe-haut et filtre secteur. Les changements s'appliquent immédiatement.

## Encodage EEG
Ajustez le chevauchement entre les époques d'embedding de 5 secondes. Plus de chevauchement = plus de vecteurs par minute.

## Calibration
Configurez la tâche de calibration : labels d'action, durées des phases, nombre de répétitions et démarrage automatique.

## Guidage vocal d'étalonnage (TTS)
Pendant l'étalonnage, l'application annonce chaque phase par son nom à l'aide de la synthèse vocale en anglais sur l'appareil. Le moteur est alimenté par KittenTTS (tract-onnx, ~30 Mo) avec phonémisation vocale. Le modèle est téléchargé depuis HuggingFace Hub lors du premier lancement et mis en cache localement - aucune donnée ne quitte votre appareil après cela. La parole se déclenche pour : le début de la session, chaque phase d'action, chaque pause ("Break. Next: ...") et la fin de la session. Nécessite espeak-ng sur PATH (brew / apt / apk install espeak-ng). Anglais uniquement.

## Raccourcis globaux
Définissez des raccourcis clavier système pour ouvrir les fenêtres Label, Recherche, Paramètres et Calibration.

## Journalisation de débogage
Activez/désactivez la journalisation par sous-système dans le fichier journal quotidien {dataDir}/logs/.

## Mises à jour
Vérifiez et installez les mises à jour. Utilise le système de mise à jour Tauri avec vérification Ed25519.

## Apparence
Choisissez un mode de couleur (Système / Clair / Sombre), activez le contraste élevé pour des bordures et du texte plus marqués, et sélectionnez un schéma de couleurs pour les graphiques EEG. Des palettes adaptées aux daltoniens sont disponibles. La langue est également modifiée ici via le sélecteur de langue.

## Objectifs
Définissez un objectif d'enregistrement quotidien en minutes. Une barre de progression apparaît sur le tableau de bord pendant la diffusion, et une notification est envoyée lorsque vous atteignez votre objectif. Le graphique des 30 derniers jours montre les jours atteints (vert), à moitié atteints (ambre), partiellement (atténué) ou manqués (vide).

## Embeddings textuels
Les labels et les requêtes de recherche sont vectorisés avec nomic-embed-text-v1.5 (~130 Mo, modèle ONNX, 768 dimensions). Le modèle est téléchargé une seule fois depuis HuggingFace Hub et mis en cache localement. Il alimente à la fois la recherche par similarité textuelle et l'index sémantique de labels utilisé par les Proactive Hooks.

## Raccourcis
Configurez les raccourcis clavier globaux pour ouvrir les fenêtres Label, Recherche, Paramètres et Calibration. Affiche aussi tous les raccourcis in-app (⌘K pour la palette, ? pour l'overlay, ⌘↵ pour soumettre). Format : ex. CmdOrCtrl+Shift+L.

# Suivi d'activité
NeuroSkill peut enregistrer en option quelle application est au premier plan et quand le clavier et la souris ont été utilisés en dernier. Les deux fonctions sont activées par défaut, entièrement locales et configurables indépendamment dans Paramètres → Suivi d'activité.

## Suivi de la fenêtre active
Un thread d'arrière-plan se réveille chaque seconde pour demander au système d'exploitation quelle application est au premier plan. Lorsque le nom de l'application ou le titre de la fenêtre change, une ligne est insérée dans activity.sqlite : le nom d'affichage de l'application, le chemin complet vers le bundle ou l'exécutable, le titre de la fenêtre active et un horodatage Unix en secondes. Si vous restez dans la même fenêtre, aucune nouvelle ligne n'est écrite. Sur macOS, le tracker utilise osascript - aucune permission Accessibilité n'est requise pour le nom et le chemin, mais le titre peut être vide pour les apps en bac à sable. Sur Linux, il utilise xdotool et xprop (session X11 requise). Sur Windows, il utilise un appel PowerShell GetForegroundWindow.

## Suivi de l'activité clavier et souris
Un hook d'entrée global (rdev) écoute tous les événements de touche et de souris/trackpad au niveau système. Il n'enregistre pas ce que vous tapez, quelles touches vous appuyez ou où le curseur se déplace - il met seulement à jour deux horodatages Unix en mémoire : un pour le dernier événement clavier et un pour le dernier événement souris/trackpad. Ces données sont écrites dans activity.sqlite toutes les 60 secondes, mais seulement si au moins une valeur a changé depuis le dernier enregistrement. L'interface reçoit un événement de mise à jour limité à une fois par seconde au maximum.

## Où les données sont stockées
Toutes les données d'activité résident dans un seul fichier SQLite : ~/.skill/activity.sqlite. Elles ne sont jamais transmises, synchronisées ou incluses dans des analyses. Deux tables : active_windows (une ligne par changement de focus, avec nom d'app, chemin, titre et horodatage) et input_activity (une ligne par flush de 60 secondes lors d'activité détectée). Les deux tables ont un index décroissant sur la colonne horodatage. Le mode journal WAL est activé. Le fichier peut être ouvert, exporté ou supprimé à tout moment avec n'importe quel navigateur SQLite.

## Permissions système requises
macOS - Le suivi de la fenêtre active (nom et chemin) ne nécessite aucune permission spéciale. Le suivi clavier/souris utilise un CGEventTap qui nécessite l'accès Accessibilité : Réglages Système → Confidentialité & Sécurité → Accessibilité → activer NeuroSkill. Sans cette permission, le hook échoue silencieusement - les horodatages restent à zéro et le reste de l'app fonctionne normalement. Linux - Nécessite une session X11. Utilise xdotool, xprop et libxtst. Windows - Aucune permission spéciale requise.

## Désactiver et supprimer les données
Les deux bascules dans Paramètres → Suivi d'activité prennent effet immédiatement - aucun redémarrage requis. Pour supprimer tout l'historique : quitter l'app, supprimer ~/.skill/activity.sqlite, puis redémarrer - une base vide est créée automatiquement. Pour supprimer des lignes de manière sélective, ouvrez le fichier dans un navigateur SQLite et utilisez DELETE FROM active_windows ou DELETE FROM input_activity.

# UMAP

## UMAP
Paramètres UMAP pour la projection 3D dans la comparaison de sessions : nombre de voisins (structure locale vs. globale), distance minimale (densité des clusters) et métrique (cosinus ou euclidienne). Plus de voisins préservent la topologie globale; moins révèlent les clusters locaux fins. Les projections s'exécutent en arrière-plan.

# Onglet Modèle EEG
Surveillez l'état de l'encodeur d'embedding EEG et de l'index vectoriel HNSW.

## État de l'encodeur
Indique si l'encodeur d'embedding EEG est chargé, le résumé de l'architecture (dimension, couches, têtes) et le chemin vers le fichier de poids .safetensors. L'encodeur fonctionne entièrement sur l'appareil via votre GPU.

## Embeddings aujourd'hui
Un compteur en direct du nombre d'époques EEG de 5 secondes intégrées dans l'index HNSW du jour.

## Paramètres HNSW
M (connexions par nœud) et ef_construction contrôlent le compromis qualité/vitesse de l'index.

## Normalisation des données
Le facteur data_norm appliqué à l'EEG brut avant l'encodage. La valeur par défaut (10) est calibrée pour les casques Muse 2 / Muse S.

# Appareils OpenBCI
Connectez et configurez n'importe quel board OpenBCI — Ganglion, Cyton, Cyton+Daisy, variantes WiFi Shield ou Galea — seul ou avec un autre appareil BCI.

## Sélection du board
Choisissez le board OpenBCI à utiliser. Ganglion (4 voies, BLE) est l'option la plus portable. Cyton (8 voies, USB série) offre plus de canaux. Cyton+Daisy double ce nombre à 16 voies. Les variantes WiFi Shield remplacent le lien USB/BLE par un flux Wi-Fi à 1 kHz. Galea (24 voies, UDP) est un board de recherche haute densité. Tous peuvent fonctionner seuls ou avec un autre appareil BCI.

## Ganglion BLE
Le Ganglion se connecte via Bluetooth Low Energy. Cliquez sur Connecter - NeuroSkillTM recherche le Ganglion le plus proche pendant la durée configurée. Maintenez le board à moins de 3-5 m, allumé (LED bleue clignotante). Un seul Ganglion peut être actif par adaptateur Bluetooth.

## Port série (Cyton / Cyton+Daisy)
Les boards Cyton communiquent via un dongle USB radio. Laissez le champ vide pour la détection automatique, ou entrez le port explicitement (/dev/cu.usbserial-... sur macOS, /dev/ttyUSB0 sur Linux, COM3 sur Windows). Branchez le dongle avant de cliquer sur Connecter. Sous Linux, ajoutez votre utilisateur au groupe dialout.

## WiFi Shield
Le WiFi Shield OpenBCI crée son propre réseau 2,4 GHz (SSID : OpenBCI-XXXX). Connectez votre ordinateur à ce réseau et entrez l'IP 192.168.4.1. Vous pouvez aussi l'intégrer à votre réseau local - entrez alors l'IP attribuée. Laissez vide pour la découverte automatique via mDNS. Le WiFi Shield diffuse à 1 kHz - réglez le filtre passe-bas à ≤ 500 Hz.

## Galea
Galea est un casque de recherche 24 voies (EEG + EMG + AUX) qui diffuse en UDP. Entrez l'adresse IP du Galea ou laissez vide pour accepter depuis n'importe quelle source. Les voies 1-8 sont EEG (analyse temps réel) ; 9-16 EMG ; 17-24 AUX. Les 24 voies sont enregistrées en CSV.

## Labels de canaux & Presets
Attribuez des noms d'électrodes 10-20 standard à chaque canal physique. Utilisez un preset (Frontal, Moteur, Occipital, Full 10-20) ou saisissez des noms personnalisés. Les canaux au-delà des 4 premiers ne sont enregistrés qu'en CSV.
