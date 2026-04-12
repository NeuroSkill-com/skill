# Synthèse vocale locale (TTS)

## Synthèse vocale locale (TTS)
NeuroSkillTM intègre un moteur de synthèse vocale anglais entièrement local. Il annonce les phases de calibration à voix haute (étiquettes d'action, pauses, fin) et peut être déclenché via l'API WebSocket ou HTTP. Toute la synthèse est locale - aucun internet nécessaire après le téléchargement du modèle de ~30 Mo.

## Fonctionnement
Prétraitement → découpage en phrases (≤400 car.) → phonémisation via libespeak-ng (bibliothèque C, en processus, voix en-us) → tokenisation (IPA → IDs) → inférence ONNX (KittenTTS : input_ids + style + speed → forme d'onde f32) → 1 s de silence → lecture rodio sur la sortie audio par défaut.

## Model
KittenML/kitten-tts-mini-0.8 depuis HuggingFace Hub. Voix : Jasper (en-us). 24 000 Hz mono float32. ONNX quantifié INT8 - CPU uniquement. Mis en cache dans ~/.cache/huggingface/hub/ après le premier téléchargement.

## Prérequis
espeak-ng doit être installé et dans le PATH - il fournit la phonémisation IPA en processus (bibliothèque C). macOS : brew install espeak-ng. Ubuntu/Debian : apt install libespeak-ng-dev. Alpine : apk add espeak-ng-dev. Fedora : dnf install espeak-ng-devel.

## Intégration de la calibration
Lorsqu'une session de calibration démarre, le moteur est préchauffé en arrière-plan (téléchargement du modèle si nécessaire). À chaque phase, la fenêtre de calibration appelle tts_speak. La parole ne bloque jamais la calibration - tous les appels TTS sont fire-and-forget.

## API - commande say
Déclenchez la synthèse vocale depuis tout script externe ou agent LLM. La commande retourne immédiatement. WebSocket : {"command":"say","text":"votre message"}. HTTP : POST /say avec body {"text":"votre message"}. CLI : curl -X POST http://localhost:<port>/say -d '{"text":"bonjour"}' -H 'Content-Type: application/json'.

## Journalisation de débogage
Activez la journalisation TTS dans Paramètres → Voix pour écrire les événements (texte, échantillons, latence) dans le fichier journal de NeuroSkillTM. Utile pour mesurer la latence et diagnostiquer les problèmes.

## Testez ici
Utilisez le widget ci-dessous pour tester le moteur TTS.
