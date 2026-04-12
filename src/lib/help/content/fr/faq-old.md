## Comment un hook se déclenche-t-il ?
Le worker compare chaque nouvel embedding EEG à des exemplaires récents de labels sélectionnés par mot-clé + similarité textuelle. Si la meilleure distance cosinus est sous votre seuil, le hook se déclenche.

## Pourquoi l'icône devient-elle rouge ?
Le Bluetooth est désactivé. Activez-le dans Réglages → Bluetooth.

## L'appli tourne mais ne se connecte jamais ?
1. Vérifiez que l'appareil BCI est allumé (Muse : maintenez jusqu'à la vibration ; Ganglion/Cyton : voyant bleu). 2. Restez à moins de 5 m. 3. Si le problème persiste, éteignez et rallumez l'appareil.

## Comment accorder la permission Bluetooth ?
Réglages → Confidentialité & Sécurité → Bluetooth → activer {app}.

## Puis-je recevoir des données EEG sur le réseau ?
Oui - métriques dérivées (~4 Hz) et statut (~1 Hz) via WebSocket. Les données brutes ne sont pas diffusées.

## Où sont sauvegardés mes enregistrements ?
Dans {dataDir}/ - fichiers CSV, SQLite, HNSW organisés par date.

## Que signifient les points de qualité du signal ?
Vert = bon contact. Jaune = moyen. Rouge = mauvais. Gris = aucun signal.

## Qu'est-ce que le filtre coupe-bande ?
Supprime le bruit 50/60 Hz du secteur de l'affichage.

## Quelles métriques sont stockées ?
Puissances de bande, scores dérivés, FAA, ratios, forme spectrale, cohérence, Hjorth, complexité, métriques PPG - par époque de 2,5 s.

## Qu'est-ce que la comparaison de sessions ?
Compare deux sessions côte à côte : puissances, scores, FAA, sommeil, UMAP.

## Qu'est-ce que le visualiseur UMAP 3D ?
Projette les embeddings EEG en 3D pour que les états cérébraux similaires se regroupent.

## Pourquoi UMAP montre-t-il un nuage aléatoire ?
UMAP s'exécute en arrière-plan. Un placeholder est affiché jusqu'à ce que la projection soit prête.

## Que sont les labels ?
Des étiquettes définies par l'utilisateur attachées aux moments pendant l'enregistrement.

## Qu'est-ce que la FAA ?
ln(AF8 α) - ln(AF7 α). Positif = motivation d'approche, négatif = retrait.

## Comment fonctionne le staging du sommeil ?
Classifie les époques en Éveil/N1/N2/N3/REM à partir des ratios de puissance de bande.

## Raccourcis clavier ?
⌘⇧O - Ouvrir {app}. ⌘⇧M - Comparer les sessions. Personnalisable dans Réglages → Raccourcis.

## Qu'est-ce que l'API WebSocket ?
API JSON sur le LAN (mDNS : _skill._tcp). Commandes : status, label, search, compare, sessions, sleep, umap, umap_poll.

## Que sont Focus / Relaxation / Engagement ?
Scores dérivés des ratios de puissance de bande, mappés sur 0-100 via sigmoïde.

## Que sont TAR, BAR, DTR ?
Ratios inter-bandes : Theta/Alpha, Beta/Alpha, Delta/Theta.

## Que sont PSE, APF, BPS, SNR ?
Caractéristiques spectrales : entropie, fréquence du pic alpha, pente, rapport signal/bruit.
