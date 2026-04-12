# Aperçu
Les Hooks proactifs déclenchent automatiquement des actions quand vos motifs EEG récents correspondent à des mots-clés ou états cérébraux spécifiques.

## Que sont les Hooks proactifs ?
Un Hook proactif surveille vos embeddings d'étiquettes EEG en temps réel. Quand la distance cosinus entre vos embeddings et ceux des mots-clés tombe sous un seuil, le hook se déclenche - envoyant une commande, notification, TTS ou événement WebSocket. Permet de créer des automatisations neuro-feedback sans code.

## Fonctionnement
L'application calcule des embeddings EEG toutes les quelques secondes et les compare aux mots-clés de chaque hook actif via la similarité cosinus sur l'index HNSW. Un temps de recharge empêche les déclenchements répétés. Tout est purement local - aucune donnée ne quitte votre machine.

## Scénarios
Chaque hook peut être limité à un scénario - Cognitif, Émotionnel, Physique ou Tous. Cognitif : concentration, distraction, fatigue mentale. Émotionnel : stress, calme, frustration. Physique : somnolence, fatigue physique. 'Tous' correspond quelle que soit la catégorie.

# Configurer un Hook
Chaque hook possède plusieurs champs contrôlant quand et comment il se déclenche.

## Nom du hook
Un nom descriptif et unique pour le hook (ex. 'Deep Work Guard', 'Calm Recovery'). Utilisé dans le journal d'historique et les événements WebSocket.

## Mots-clés
Mots-clés ou courtes phrases décrivant l'état cérébral à détecter (ex. 'focus', 'deep work', 'stress', 'fatigué'). Intégrés par le même modèle sentence-transformer que vos étiquettes EEG. Le hook se déclenche quand les embeddings EEG récents sont proches.

## Suggestions de mots-clés
L'application suggère des termes depuis votre historique d'étiquettes - correspondance floue et sémantique. Badge de source : 'flou', 'sémantique' ou les deux. ↑/↓ et Entrée pour accepter rapidement.

## Seuil de distance
Distance cosinus maximale (0-1). Valeurs basses = strict, hautes = tolérant. Typique : 0,08 (très strict) à 0,25 (souple). Commencez à 0,12-0,16 et ajustez avec l'outil de suggestion.

## Outil de suggestion de distance
Analyse vos données EEG enregistrées par rapport aux mots-clés du hook. Calcule la distribution (min, p25, p50, p75, max) et recommande un seuil. Barre de percentiles visuelle. 'Appliquer' pour utiliser la valeur suggérée.

## Références récentes
Nombre d'échantillons EEG récents à comparer (défaut : 12). Valeurs hautes = lissage des pics, valeurs basses = réactivité accrue. Plage : 10-20.

## Commande
Chaîne optionnelle diffusée dans l'événement WebSocket (ex. 'focus_reset', 'calm_breath'). Les outils d'automatisation externes peuvent y réagir pour déclencher des actions ou scripts.

## Texte de charge utile
Message optionnel lisible inclus dans l'événement de déclenchement (ex. 'Prenez une pause de 2 minutes.'). Affiché dans les notifications et prononçable via TTS si le guidage vocal est activé.

# Avancé
Conseils, historique et intégration avec des outils externes.

## Exemples rapides
Modèles prêts à l'emploi : Deep Work Guard (concentration cognitive), Calm Recovery (stress émotionnel), Body Break (fatigue physique). Cliquez pour ajouter avec des valeurs pré-remplies. Ajustez à vos motifs EEG personnels.

## Historique des déclenchements
Journal repliable enregistrant chaque déclenchement : horodatage, étiquette, distance cosinus, commande, mots-clés. Pour auditer le comportement, vérifier les seuils et déboguer les faux positifs. Pagination intégrée.

## Événements WebSocket
Un événement JSON est diffusé via l'API WebSocket contenant : nom du hook, commande, texte, étiquette, distance, horodatage. Les clients externes peuvent écouter pour créer des automatisations - tamiser les lumières, pausser la musique, envoyer un message Slack, etc.

## Conseils d'ajustement
Commencez avec un hook et quelques mots-clés correspondant à des étiquettes déjà enregistrées. Utilisez l'outil de suggestion pour le seuil initial. Surveillez l'historique pendant un jour. Baissez le seuil pour les faux positifs, augmentez s'il ne se déclenche jamais. Des mots-clés spécifiques améliorent la précision.
