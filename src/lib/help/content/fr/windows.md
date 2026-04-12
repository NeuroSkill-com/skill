# Fenêtres
{app} utilise des fenêtres séparées pour des tâches spécifiques. Chacune peut être ouverte depuis le menu contextuel ou via un raccourci clavier.

## 🏷  Fenêtre Label
Ouverte via le menu, un raccourci global ou le bouton tag. Tapez un label pour annoter le moment EEG actuel. Soumettez avec Ctrl/⌘+Entrée. Échap pour annuler.

## 🔍  Fenêtre Recherche
La fenêtre Recherche propose trois modes - Similarité EEG, Texte et Interactif - interrogeant vos données de différentes façons.

## Recherche par similarité EEG
Choisissez une plage de dates/heures et lancez une recherche par plus proches voisins sur toutes les embeddings ZUNA enregistrées dans cette fenêtre. L'index HNSW renvoie les k époques EEG de 5 secondes les plus similaires de tout votre historique, classées par distance cosinus. Distance plus faible = état cérébral plus similaire. Les étiquettes qui chevauchent un horodatage de résultat sont affichées en ligne.

## Recherche par embedding textuel
Saisissez n'importe quel concept, activité ou état mental en langage naturel (p. ex. « concentration profonde », « anxieux », « méditation yeux fermés »). La requête est vectorisée par le même modèle sentence-transformer que celui utilisé pour l'indexation des étiquettes, puis comparée à toutes vos annotations par similarité cosinus dans l'index HNSW. Les résultats sont vos propres étiquettes classées par proximité sémantique - pas par correspondance de mots-clés. Un graphe kNN 3D visualise la structure de voisinage.

## Recherche interactive multimodale
Saisissez un concept en texte libre et {app} exécute un pipeline multimodal en quatre étapes : (1) la requête est vectorisée ; (2) les text-k étiquettes sémantiquement les plus similaires sont récupérées ; (3) pour chaque étiquette, {app} calcule l'embedding EEG moyen sur sa fenêtre d'enregistrement et recherche les eeg-k époques EEG les plus similaires ; (4) pour chaque voisin EEG, les annotations dans ±reach minutes sont collectées comme « étiquettes trouvées ». Le résultat est un graphe dirigé à quatre couches - Requête → Correspondances texte → Voisins EEG → Étiquettes trouvées - exportable en SVG ou en DOT Graphviz.

## 🎯  Fenêtre Calibration
Exécute une tâche de calibration guidée : phases d'action alternées avec des pauses. Nécessite un appareil BCI connecté et en streaming.

## ⚙  Fenêtre Paramètres
Quatre onglets : Paramètres, Raccourcis (raccourcis globaux, palette de commandes, touches intégrées), Modèle EEG (encodeur & statut HNSW). Ouvrir depuis le menu ou le bouton engrenage.

## ?  Fenêtre d'aide
Cette fenêtre. Une référence complète pour chaque partie de l'interface {app}.

## 🧭  Assistant de configuration
Un assistant en cinq étapes pour la première utilisation : appairage Bluetooth, ajustement du casque et première calibration. S'ouvre automatiquement au premier lancement ; peut être rouvert depuis la palette de commandes (⌘K → Assistant de configuration).

## 🌐  Fenêtre Statut API
Un tableau de bord en temps réel montrant tous les clients WebSocket actuellement connectés et un journal de requêtes défilable. Affiche le port du serveur, le protocole et les infos de découverte mDNS. Inclut des extraits de connexion rapide pour ws:// et dns-sd. Actualisation automatique toutes les 2 secondes. Ouvrir depuis le menu ou la palette de commandes.

## 🌙 Stades de sommeil
Pour les sessions de 30 minutes ou plus, la vue Historique affiche un hypnogramme généré automatiquement. Note : les casques BCI grand public comme Muse utilisent 4 électrodes sèches - le staging est approximatif, ce n'est pas un polysomnographe clinique.

## ⚖  Comparer
Choisissez deux plages horaires sur la chronologie et comparez leurs distributions de puissance de bande, scores de relaxation/engagement et FAA côte à côte. Inclut les stades de sommeil, les métriques avancées et Brain NebulaTM - une projection UMAP 3D montrant la similarité des deux périodes dans l'espace EEG haute dimension. Ouvrir depuis le menu tray ou la palette de commandes (⌘K → Comparer).

# Overlays & Palette de commandes
Overlays d'accès rapide disponibles dans chaque fenêtre via des raccourcis clavier.

## ⌨  Palette de commandes (⌘K / Ctrl+K)
Un menu déroulant rapide listant toutes les actions exécutables de l'app. Tapez pour filtrer, ↑↓ pour naviguer, Entrée pour exécuter. Disponible dans chaque fenêtre. Les commandes incluent l'ouverture de fenêtres (Paramètres, Aide, Recherche, Label, Historique, Calibration), les actions appareil (réessayer la connexion, paramètres Bluetooth) et les utilitaires (afficher les raccourcis, vérifier les mises à jour).

## ?  Overlay des raccourcis clavier
Appuyez sur ? dans n'importe quelle fenêtre (hors champs texte) pour afficher un overlay flottant listant tous les raccourcis clavier - raccourcis globaux configurés dans Paramètres → Raccourcis, plus les touches intégrées comme ⌘K pour la palette et ⌘Entrée pour soumettre les labels. Appuyez à nouveau sur ? ou Échap pour fermer.
