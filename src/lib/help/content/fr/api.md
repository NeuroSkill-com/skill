# Aperçu

## Streaming en direct
{app} diffuse des métriques EEG dérivées et l'état de l'appareil via un serveur WebSocket local. Événements : eeg-bands (~4 Hz - 60+ scores), device-status (~1 Hz - batterie, état de connexion), label-created. Les échantillons bruts EEG/PPG/IMU ne sont pas diffusés via l'API WebSocket.

## Commandes
Les clients peuvent envoyer des commandes JSON via WebSocket : status, calibrate, label, search, sessions, compare, sleep, umap/umap_poll. Réponses en JSON avec booléen "ok".

# Référence des commandes

## status
_(aucun)_

Retourne l'état de l'appareil, les infos de session, les compteurs d'embeddings et la qualité du signal.

## calibrate
_(aucun)_

Ouvre la fenêtre de calibration. Nécessite un appareil connecté.

## label
text (chaîne, requis) ; label_start_utc (u64, optionnel)

Insère un label horodaté dans la base de données.

## search
start_utc, end_utc (u64, requis) ; k, ef (u64, optionnel)

Recherche les k plus proches voisins dans l'index HNSW.

## compare
a_start_utc, a_end_utc, b_start_utc, b_end_utc (u64, requis)

Compare deux plages temporelles en renvoyant les métriques agrégées de puissance de bande (puissances relatives, scores focus/relaxation/engagement et FAA) pour chacune. Retourne { a: SessionMetrics, b: SessionMetrics }.

## sessions
_(aucun)_

Liste toutes les sessions d'embeddings des bases quotidiennes. Plages d'enregistrement contiguës (écart > 2 min = nouvelle session). Plus récentes en premier.

## sleep
start_utc, end_utc (u64, requis)

Classifie chaque époque en stade de sommeil (Éveil/N1/N2/N3/REM). Retourne hypnogramme + résumé.

## umap
a_start_utc, a_end_utc, b_start_utc, b_end_utc (u64, requis)

Met en file d'attente une projection UMAP 3D. Retourne un job_id pour interrogation. Non-bloquant.

## umap_poll
job_id (string, requis)

Interroge le résultat d'un job UMAP. Retourne { status: pending | done, points?: [...] }.

## say
text : string (requis)

Prononcer du texte via le TTS local. Fire-and-forget - retourne immédiatement pendant que l'audio est lu en arrière-plan. Initialise le moteur au premier appel.
