# Aperçu
NeuroSkill intègre un serveur LLM local optionnel offrant un assistant IA privé compatible OpenAI sans envoyer de données au cloud.

## Qu'est-ce que la fonctionnalité LLM ?
La fonctionnalité LLM intègre un serveur d'inférence basé sur llama.cpp. Il sert des points de terminaison compatibles OpenAI (/v1/chat/completions, /v1/completions, /v1/embeddings, /v1/models, /health) sur le même port que l'API WebSocket. Tout client compatible OpenAI peut s'y connecter.

## Confidentialité et utilisation hors ligne
Toute l'inférence s'exécute localement. Aucune donnée ne quitte localhost. Seul le téléchargement initial du modèle depuis HuggingFace Hub nécessite internet. Après mise en cache, vous pouvez être entièrement hors ligne.

## API compatible OpenAI
Le serveur parle le même protocole que l'API OpenAI. Toute bibliothèque avec paramètre base_url (openai-python, openai-node, LangChain, LlamaIndex) fonctionne directement. Définissez base_url sur http://localhost:<port>/v1.

# Gestion des modèles
Parcourir, télécharger et activer des modèles de langage quantifiés GGUF depuis le catalogue intégré.

## Catalogue de modèles
Le catalogue liste des familles de modèles (Qwen, Llama, Gemma, Phi) avec plusieurs variantes de quantification. Utilisez le menu déroulant pour parcourir et choisir une quantification. Les modèles marqués ★ sont recommandés.

## Niveaux de quantification
Chaque modèle est disponible en plusieurs niveaux GGUF (Q4_K_M, Q5_K_M, Q6_K, Q8_0, etc.). Les quantifications basses sont plus petites et rapides mais sacrifient de la qualité. Q4_K_M est le meilleur compromis. Q8_0 est quasi sans perte mais nécessite deux fois plus de mémoire. BF16/F16/F32 sont non quantifiés.

## Badges de compatibilité matérielle
Chaque ligne affiche un badge coloré : 🟢 Très bien - tient en VRAM GPU avec marge. 🟡 Bien - marge serrée. 🟠 Serré - déchargement CPU possible. 🔴 Ne tient pas. L'estimation prend en compte VRAM, RAM, taille du modèle et surcoût de contexte.

## Modèles vision / multimodaux
Les familles Vision/Multimodal incluent un projecteur multimodal optionnel (mmproj). Téléchargez les deux pour activer l'entrée d'images dans le chat. Le projecteur étend le modèle texte - ce n'est pas un modèle autonome.

## Téléchargement et suppression
Cliquez 'Télécharger' pour récupérer un modèle depuis HuggingFace Hub. Une barre de progression affiche l'état en temps réel. Annulation possible à tout moment. Les modèles téléchargés sont stockés localement et peuvent être supprimés. Utilisez 'Actualiser le cache' pour rescanner le catalogue.

# Paramètres d'inférence
Ajustez la façon dont le serveur charge et exécute les modèles.

## Couches GPU
Nombre de couches transformer déchargées vers le GPU. 'Toutes' pour vitesse maximale, 0 pour CPU uniquement. Les valeurs intermédiaires répartissent le modèle entre GPU et CPU - utile quand le modèle dépasse légèrement la VRAM.

## Taille du contexte
Taille du cache KV en tokens. 'Auto' choisit le plus grand contexte qui tient dans votre GPU/RAM en fonction de la taille et de la quantification du modèle. Des contextes plus grands mémorisent plus d'historique mais consomment plus de mémoire. Les options sont limitées au maximum entraîné du modèle. En cas d'erreurs mémoire, réduisez la taille du contexte.

## Requêtes parallèles
Nombre maximal de boucles de décodage simultanées. Plus de clients peuvent partager le serveur mais la mémoire en pic augmente. 1 suffit pour un utilisateur unique.

## Clé API
Token Bearer optionnel requis pour chaque requête /v1/*. Laissez vide pour un accès ouvert sur localhost. Définissez une clé pour restreindre l'accès sur un réseau local.

# Outils intégrés
Le chat LLM peut appeler des outils locaux pour collecter des informations ou effectuer des actions en votre nom.

## Fonctionnement des outils
Le modèle peut demander à appeler des outils pendant une conversation. L'application les exécute localement et renvoie le résultat. Les outils ne sont invoqués que sur demande explicite du modèle - ils ne s'exécutent jamais en arrière-plan.

## Outils sûrs
Date, Localisation, Recherche web, Récupération web et Lecture de fichier sont en lecture seule. Date retourne la date/heure locale. Localisation fournit une géolocalisation IP approximative. Recherche web interroge DuckDuckGo. Récupération web récupère le texte d'une URL. Lecture de fichier lit des fichiers locaux avec pagination.

## Outils privilégiés (⚠️)
Bash, Écriture de fichier et Édition de fichier peuvent modifier votre système. Bash exécute des commandes shell. Écriture de fichier crée ou écrase des fichiers. Édition de fichier effectue des rechercher-remplacer. Désactivés par défaut avec badge d'avertissement. N'activez que si vous comprenez les risques.

## Mode d'exécution et limites
Mode parallèle : appels simultanés (plus rapide). Mode séquentiel : un par un (plus sûr). 'Max tours' limite les allers-retours outil/résultat par message. 'Max appels par tour' plafonne les invocations simultanées.

# Chat et journaux
Interagissez avec le modèle et surveillez l'activité du serveur.

## Fenêtre de chat
Ouvrez le chat depuis la carte du serveur LLM ou le menu système. Interface avec rendu Markdown, coloration du code et visualisation des appels d'outils. Conversations éphémères - non sauvegardées. Les modèles vision acceptent les images par glisser-déposer.

## Utiliser des clients externes
Pointez tout frontend compatible OpenAI vers http://localhost:<port>/v1, définissez une clé API si configurée et sélectionnez un modèle depuis /v1/models. Options populaires : Open WebUI, Chatbot UI, Continue (VS Code), curl/httpie.

## Journaux du serveur
Le visualiseur diffuse la sortie du serveur en temps réel : progression du chargement, vitesse de génération, erreurs. Mode 'Verbeux' pour les diagnostics détaillés de llama.cpp. Défilement automatique, pausable en faisant défiler manuellement.
