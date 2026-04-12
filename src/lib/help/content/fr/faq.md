## Où sont stockées mes données ?
Tout est stocké localement dans {dataDir}/ - enregistrements CSV, index HNSW, bases SQLite, labels, journaux et paramètres.

## Que fait l'encodeur ZUNA ?
ZUNA est l'un des plusieurs backends d'embedding EEG disponibles dans {app}. C'est un encodeur transformer accéléré par GPU qui convertit des époques EEG de 5 secondes en vecteurs d'embedding compacts. Ces vecteurs capturent la signature neuronale de chaque moment et alimentent la fonction de recherche par similarité. Les autres backends incluent LUNA et NeuroRVQ.

## Pourquoi la calibration nécessite-t-elle un appareil connecté ?
La calibration enregistre des données EEG labellisées. Sans données en streaming, il n'y aurait pas de signal neural à associer.

## Comment me connecter depuis Python / Node.js ?
Découvrez le port WebSocket via mDNS, puis ouvrez une connexion WebSocket standard. Voir l'onglet API.

## Que signifient les indicateurs de qualité du signal ?
Chaque point représente une électrode EEG. Vert = bon contact. Jaune = artefact. Rouge = mauvais contact. Gris = pas de signal.

## Puis-je changer la fréquence du filtre secteur ?
Oui - allez dans Paramètres → Traitement du signal et choisissez 50 Hz (Europe) ou 60 Hz (Amériques, Japon).

## Comment réinitialiser un appareil appairé ?
Ouvrez Paramètres → Appareils appariés, puis cliquez sur le bouton × pour oublier l'appareil.

## Pourquoi l'icône de la barre devient-elle rouge ?
Le Bluetooth est désactivé. Ouvrez Préférences Système → Bluetooth et activez-le.

## L'application tourne en boucle sans se connecter - que faire ?
1. Vérifiez que l'appareil est allumé (Muse : maintenez jusqu'à la vibration ; Ganglion/Cyton : voyant bleu). 2. Restez à moins de 5 m. 3. Si le problème persiste, éteignez et rallumez l'appareil.

## Pourquoi mon appareil s'est-il déconnecté automatiquement ?
Si aucune donnée n'arrive pendant 30 secondes après la réception d'au moins une trame EEG, {app} considère l'appareil comme silencieusement déconnecté (par ex. hors de portée BLE ou éteint sans déconnexion propre). L'icône de la barre repasse en gris et le scan reprend automatiquement.

## Comment accorder la permission Bluetooth ?
macOS affiche une boîte de dialogue de permission. Si vous l'avez rejetée, allez dans Préférences Système → Confidentialité → Bluetooth.

## Quelles métriques sont stockées dans la base de données ?
Chaque époque de 2,5 s stocke : le vecteur d'embedding EEG, les puissances de bande relatives (delta, thêta, alpha, bêta, gamma, high-gamma) moyennées sur les canaux, les puissances de bande par canal en JSON, les scores dérivés (relaxation, engagement), la FAA, les ratios inter-bandes (TAR, BAR, DTR), la forme spectrale (PSE, APF, BPS, SNR), la cohérence, la suppression Mu, l'indice d'humeur et les moyennes PPG si disponibles.

## Qu'est-ce que la comparaison de sessions ?
Comparer (⌘⇧M) compare deux plages horaires côte à côte : barres de puissance avec écarts, tous les scores et ratios, FAA, hypnogrammes et Brain NebulaTM - une projection UMAP 3D.

## Qu'est-ce que Brain NebulaTM ?
Brain NebulaTM (techniquement : UMAP Embedding Distribution) projette les embeddings EEG haute dimension en 3D : les états cérébraux similaires apparaissent proches. Plage A (bleu) et B (ambre) forment des clusters distincts. Vous pouvez orbiter, zoomer et cliquer sur les points étiquetés pour voir les connexions temporelles.

## Pourquoi Brain NebulaTM affiche-t-il un nuage aléatoire au début ?
La projection UMAP est coûteuse en calcul et s'exécute en file d'attente en arrière-plan. Un nuage aléatoire est affiché en attendant. Les points s'animent vers leurs positions finales une fois le calcul terminé.

## Que sont les labels et comment les utiliser ?
Les labels sont des tags (ex. « méditation », « lecture ») associés à un moment. Dans le visualiseur UMAP, les points étiquetés sont plus grands avec des anneaux colorés - cliquez pour suivre un label dans le temps.

## Qu'est-ce que l'asymétrie alpha frontale (FAA) ?
FAA = ln(AF8 α) - ln(AF7 α). Valeurs positives = motivation d'approche. Valeurs négatives = retrait (évitement, anxiété).

## Comment fonctionne la détection des stades de sommeil ?
Chaque époque EEG est classée en Éveil, N1, N2, N3 ou REM selon les puissances relatives delta, thêta, alpha et bêta. La vue comparée montre un hypnogramme par session.

## Quels sont les raccourcis clavier ?
⌘⇧O - Ouvrir {app}. ⌘⇧M - Comparaison de sessions. Personnalisable dans Réglages → Raccourcis.

## Qu'est-ce que l'API WebSocket ?
{app} expose une API WebSocket JSON sur le réseau local (mDNS : _skill._tcp). Commandes : status, label, search, compare, sessions, sleep, umap (mettre en file), umap_poll (récupérer le résultat).

## Que sont les scores Focus, Relaxation et Engagement ?
Focus = β/(α+θ), Relaxation = α/(β+θ), Engagement = β/(α+θ) avec courbe plus douce. Tous mappés de 0 à 100 via sigmoïde.

## Que sont TAR, BAR et DTR ?
TAR (Thêta/Alpha) - plus élevé = plus somnolent. BAR (Bêta/Alpha) - plus élevé = plus stressé/concentré. DTR (Delta/Thêta) - plus élevé = sommeil plus profond.

## Que sont PSE, APF, BPS et SNR ?
PSE (Entropie Spectrale, 0-1) - complexité spectrale. APF (Fréquence de Pic Alpha, Hz). BPS (Pente de Puissance) - exposant 1/f. SNR (Rapport Signal/Bruit, dB).

## Qu'est-ce que le rapport Thêta/Bêta (TBR) ?
Le TBR est le rapport de la puissance absolue thêta sur bêta. Des valeurs plus élevées indiquent une activation corticale réduite - un TBR élevé est associé à la somnolence et à la dysrégulation attentionnelle. Réf. : Angelidis et al. (2016).

## Que sont les paramètres de Hjorth ?
Trois caractéristiques temporelles de Hjorth (1970) : Activité (variance du signal / puissance totale), Mobilité (estimation de la fréquence moyenne) et Complexité (largeur de bande / écart par rapport à une sinusoïde pure). Ils sont peu coûteux en calcul et largement utilisés dans les pipelines ML d'EEG.

## Quelles mesures de complexité non linéaire sont calculées ?
Quatre mesures : Entropie de Permutation (complexité des motifs ordinaux, Bandt & Pompe 2002), Dimension Fractale de Higuchi (structure fractale du signal, Higuchi 1988), Exposant DFA (corrélations temporelles longue portée, Peng et al. 1994) et Entropie d'Échantillon (régularité du signal, Richman & Moorman 2000). Toutes moyennées sur les 4 canaux EEG.

## Que sont SEF95, Centroïde Spectral, CAP et Indice de Latéralité ?
SEF95 (Fréquence de Bord Spectral) est la fréquence en dessous de laquelle se trouve 95% de la puissance totale - utilisée en anesthésie. Le Centroïde Spectral est la fréquence moyenne pondérée par la puissance (indicateur d'éveil). Le CAP (Couplage Phase-Amplitude) mesure l'interaction thêta-gamma associée à l'encodage mémoriel. L'Indice de Latéralité est l'asymétrie gauche/droite généralisée sur toutes les bandes.

## Quelles métriques PPG sont calculées ?
Sur Muse 2/S (avec capteur PPG) : fréquence cardiaque (bpm), RMSSD/SDNN/pNN50 (variabilité de la fréquence cardiaque - tonus parasympathique), rapport LF/HF (équilibre sympathovagal), fréquence respiratoire (à partir de l'enveloppe PPG), estimation SpO2 (non calibrée), indice de perfusion (flux sanguin périphérique) et indice de stress de Baevsky (stress autonome).

## Comment utiliser le minuteur de concentration ?
Ouvrez le minuteur via le menu de la zone de notification, la palette de commandes (⌘K → « Minuteur de concentration ») ou le raccourci global (⌘⇧P par défaut). Choisissez un préréglage - Pomodoro (25/5), Travail en profondeur (50/10) ou Concentration courte (15/5) - ou définissez des durées personnalisées. Activez « Étiquetage EEG auto » pour que NeuroSkillTM marque automatiquement les enregistrements EEG au début et à la fin de chaque phase de concentration. Vos paramètres sont sauvegardés automatiquement.

## Comment gérer ou modifier mes annotations ?
Ouvrez la fenêtre Étiquettes via la palette de commandes (⌘K → « Toutes les étiquettes »). Elle affiche toutes les annotations avec édition de texte en ligne (cliquer sur une étiquette, ⌘↵ pour sauvegarder ou Échap pour annuler), suppression (avec confirmation) et métadonnées indiquant la plage temporelle EEG. Utilisez la barre de recherche pour filtrer. Les étiquettes sont paginées par 50 pour les grandes archives.

## Comment comparer deux sessions côte à côte ?
Depuis la page Historique, cliquez sur « Comparaison rapide » pour activer le mode comparaison. Des cases à cocher apparaissent sur chaque ligne de session - sélectionnez exactement deux, puis cliquez sur « Comparer la sélection ». Vous pouvez aussi ouvrir Comparer depuis la zone de notification ou la palette de commandes et utiliser les menus déroulants.

## Comment fonctionne la recherche par embedding textuel ?
Votre requête est convertie en vecteur par le même modèle sentence-transformer qui indexe vos étiquettes. Ce vecteur est ensuite recherché dans l'index HNSW par recherche de plus proches voisins approximative. Les résultats sont vos propres annotations classées par similarité sémantique - chercher « calme et concentré » fait remonter des étiquettes comme « lecture profonde » ou « méditation » même si ces mots n'apparaissent pas dans votre requête. Nécessite le modèle d'embedding téléchargé et l'index d'étiquettes construit (Paramètres → Embeddings).

## Comment fonctionne la recherche interactive multimodale ?
La recherche interactive relie texte, EEG et temps en une seule requête. Étape 1 : la requête textuelle est vectorisée. Étape 2 : les text-k étiquettes sémantiquement les plus proches sont trouvées. Étape 3 : pour chaque étiquette, {app} calcule l'embedding EEG moyen sur sa fenêtre d'enregistrement et récupère les eeg-k époques EEG les plus proches dans tous les index journaliers - passant du langage à l'espace état-cerveau. Étape 4 : pour chaque moment EEG trouvé, les annotations dans ±reach minutes sont collectées comme « étiquettes trouvées ». Les quatre couches de nœuds (Requête → Correspondances texte → Voisins EEG → Étiquettes trouvées) sont rendues en graphe dirigé. Exportable en SVG ou en source DOT.

## Comment déclencher la synthèse vocale TTS depuis un script ?
Utilisez l'API WebSocket ou HTTP. WebSocket : {"command":"say","text":"votre message"}. HTTP : curl -X POST http://localhost:<port>/say -H 'Content-Type: application/json' -d '{"text":"votre message"}'. Fire-and-forget - répond immédiatement.

## Pourquoi n'y a-t-il aucun son du TTS ?
Vérifiez que espeak-ng est installé et dans le PATH. Vérifiez votre sortie audio. Au premier lancement, le modèle (~30 Mo) doit être téléchargé. Activez la journalisation TTS dans Paramètres → Voix.

## Puis-je changer la voix ou la langue du TTS ?
La version actuelle utilise la voix Jasper (en-us) du modèle KittenML/kitten-tts-mini-0.8. Seul le texte anglais est phonémisé correctement. Des voix et langues supplémentaires sont prévues.

## La synthèse vocale nécessite-t-elle une connexion internet ?
Seulement une fois, pour le téléchargement initial du modèle (~30 Mo) depuis HuggingFace Hub. Ensuite, toute la synthèse fonctionne entièrement hors ligne. Le modèle est mis en cache dans ~/.cache/huggingface/hub/ et réutilisé à chaque lancement.

## Quels boards OpenBCI NeuroSkillTM prend-il en charge ?
NeuroSkill™ prend en charge tous les boards de l'écosystème OpenBCI : Ganglion (4 voies, BLE), Ganglion + WiFi Shield (4 voies, 1 kHz), Cyton (8 voies, dongle USB), Cyton + WiFi Shield (8 voies, 1 kHz), Cyton+Daisy (16 voies, USB), Cyton+Daisy + WiFi Shield (16 voies, 1 kHz) et Galea (24 voies, UDP). Tous peuvent fonctionner avec un autre appareil BCI. Sélectionnez le board dans Paramètres → OpenBCI, puis cliquez sur Connecter.

## Comment connecter le Ganglion via Bluetooth ?
1. Allumez le Ganglion - la LED bleue doit clignoter lentement. 2. Dans Paramètres → OpenBCI, sélectionnez « Ganglion - 4 voies · BLE ». 3. Sauvegardez, puis cliquez sur Connecter. NeuroSkillTM recherche pendant la durée configurée (10 s par défaut). Maintenez le board à moins de 3-5 m. Sur macOS, accordez la permission Bluetooth lors du premier usage.

## Mon Ganglion est allumé mais NeuroSkillTM ne le trouve pas - que faire ?
1. Vérifiez que la LED bleue clignote (si elle est fixe ou éteinte, appuyez sur le bouton pour réveiller). 2. Augmentez le timeout BLE dans Paramètres → OpenBCI. 3. Rapprochez le board à moins de 2 m. 4. Quittez et relancez NeuroSkillTM pour réinitialiser l'adaptateur BLE. 5. Désactivez puis réactivez le Bluetooth. 6. Assurez-vous qu'aucune autre application (OpenBCI GUI) n'est déjà connectée - BLE n'autorise qu'une seule connexion centrale à la fois. 7. Sur macOS 14+, vérifiez la permission Bluetooth dans Réglages Système → Confidentialité.

## Comment connecter un Cyton via USB ?
1. Branchez le dongle USB radio sur votre ordinateur. 2. Allumez le Cyton (interrupteur sur PC). 3. Dans Paramètres → OpenBCI, sélectionnez « Cyton - 8 voies · USB série ». 4. Cliquez sur Actualiser pour lister les ports, puis sélectionnez le bon (/dev/cu.usbserial-... sur macOS, /dev/ttyUSB0 sur Linux, COM3 sur Windows) ou laissez vide pour la détection automatique. 5. Sauvegardez et cliquez sur Connecter.

## Le port série n'apparaît pas ou j'obtiens une erreur de permission - comment résoudre ?
macOS : le dongle apparaît en /dev/cu.usbserial-*. Si absent, installez le pilote CP210x ou FTDI VCP. Linux : exécutez sudo usermod -aG dialout $USER puis déconnectez-vous et reconnectez-vous. Vérifiez que /dev/ttyUSB0 apparaît après le branchement. Windows : installez le pilote CP2104 USB-UART ; le port COM apparaîtra dans le Gestionnaire de périphériques → Ports.

## Comment utiliser le WiFi Shield OpenBCI ?
1. Empilez le WiFi Shield sur le Cyton ou le Ganglion et allumez le board. 2. Connectez votre ordinateur au réseau WiFi du shield (SSID : OpenBCI-XXXX). 3. Dans Paramètres, sélectionnez la variante WiFi correspondante. 4. Entrez l'IP 192.168.4.1 ou laissez vide pour la découverte automatique. 5. Cliquez sur Connecter. Le WiFi Shield diffuse à 1000 Hz - réglez le filtre passe-bas à ≤ 500 Hz.

## Qu'est-ce que le board Galea et comment le configurer ?
Galea d'OpenBCI est un casque biosignaux 24 voies (EEG + EMG + AUX) diffusant en UDP. 1. Allumez Galea et connectez-le à votre réseau local. 2. Sélectionnez « Galea - 24 voies · UDP » dans Paramètres. 3. Entrez l'adresse IP ou laissez vide. 4. Cliquez sur Connecter. Les voies 1-8 sont EEG ; 9-16 EMG ; 17-24 AUX. Les 24 voies sont enregistrées en CSV.

## Puis-je utiliser deux appareils BCI simultanément ?
Oui — NeuroSkill™ peut diffuser depuis les deux simultanément. L'appareil connecté en premier pilote le tableau de bord en direct, l'affichage des puissances de bande et le pipeline d'embedding EEG. Les données du second appareil sont enregistrées en CSV pour l'analyse hors ligne. L'analyse multi-appareils simultanée dans le pipeline temps réel est prévue pour une version future.

## Seulement 4 des 8 voies du Cyton sont utilisées en temps réel - pourquoi ?
Le pipeline d'analyse en temps réel (filtres, puissances de bande, embeddings EEG, points de qualité du signal) est actuellement conçu pour des entrées à 4 canaux, correspondant au format du casque Muse. Pour le Cyton (8 voies) et le Cyton+Daisy (16 voies), les canaux 1 à 4 alimentent le pipeline en direct ; tous les canaux sont écrits en CSV pour le travail hors ligne. La prise en charge complète du pipeline multi-canaux est prévue.

## Comment améliorer la qualité du signal sur un board OpenBCI ?
1. Appliquez du gel conducteur sur chaque électrode et écartez les cheveux pour un contact direct avec le cuir chevelu. 2. Vérifiez l'impédance avec OpenBCI GUI (objectif : < 20 kΩ). 3. Connectez l'électrode SRB sur le mastoïde (derrière l'oreille). 4. Utilisez des câbles courts à l'écart des alimentations. 5. Activez le filtre coupe-bande dans Paramètres → Traitement du signal (50 Hz en Europe). 6. Pour le Ganglion BLE : éloignez le board des ports USB 3.0 qui émettent sur 2,4 GHz.

## {app} prend-il en charge le bandeau AWEAR ?
Oui. AWEAR est un appareil EEG BLE à canal unique échantillonnant à 256 Hz. La connexion fonctionne comme pour les autres appareils BLE — allumez le bandeau, accordez la permission Bluetooth si demandé, et {app} le découvrira et se connectera automatiquement. Le canal EEG unique alimente le pipeline d'analyse en temps réel.

## La connexion OpenBCI se coupe fréquemment - comment la stabiliser ?
Ganglion BLE : maintenez le board à moins de 2 m ; branchez l'adaptateur BLE sur un port USB 2.0 (l'USB 3.0 perturbe le 2,4 GHz). Cyton USB : utilisez un câble USB court et de qualité, directement sur l'ordinateur. WiFi Shield : évitez que le canal 2,4 GHz du shield ne chevauche celui du routeur. En général : évitez les applications gourmandes en sans-fil (visioconférence, sync de fichiers) pendant l'enregistrement.

## Que capture exactement le suivi d'activité ?
Le suivi de fenêtre active écrit une ligne dans activity.sqlite à chaque changement d'application ou de titre de fenêtre : nom d'affichage de l'application, chemin complet vers le bundle ou l'exécutable, titre de la fenêtre (peut être vide pour les apps en bac à sable) et horodatage Unix en secondes. Le suivi clavier/souris écrit un échantillon périodique toutes les 60 secondes, mais seulement s'il y a eu de l'activité depuis le dernier enregistrement : deux horodatages - dernier événement clavier et dernier événement souris/trackpad. Il n'enregistre jamais les frappes, le texte tapé, les positions du curseur ni les cibles de clic.

## Pourquoi macOS demande-t-il l'accès Accessibilité pour le suivi des entrées ?
Le suivi clavier/souris utilise un CGEventTap - une API macOS qui intercepte les événements d'entrée globaux. Apple exige la permission Accessibilité pour toute application lisant des entrées globales. Sans elle, le tap échoue silencieusement : NeuroSkill continue de fonctionner normalement, mais les horodatages restent à zéro. Pour accorder l'accès : Réglages Système → Confidentialité & Sécurité → Accessibilité → NeuroSkill → activer. Si vous préférez ne pas l'accorder, désactivez simplement « Suivi de l'activité clavier et souris » dans les Paramètres - cela empêche l'installation du hook.

## Comment effacer les données de suivi d'activité ?
Toutes les données de suivi d'activité résident dans un seul fichier : ~/.skill/activity.sqlite. Pour tout supprimer : quitter NeuroSkill, supprimer ce fichier, puis redémarrer - une base vide est créée automatiquement. Pour arrêter la collecte future sans toucher aux données existantes, désactivez les deux bascules dans Paramètres → Suivi d'activité (effet immédiat). Pour supprimer des lignes de manière sélective, ouvrez le fichier dans un navigateur SQLite et utilisez DELETE FROM active_windows ou DELETE FROM input_activity.

## Pourquoi {app} demande-t-il l'autorisation Accessibilité sur macOS ?
{app} utilise l'API macOS CGEventTap pour enregistrer le dernier horodatage d'une pression de touche ou d'un mouvement de souris. Cela sert à calculer les horodatages d'activité affichés dans le panneau Suivi d'activité. Seuls les horodatages sont stockés - aucune frappe, aucune position du curseur. La fonctionnalité se désactive silencieusement si la permission n'est pas accordée.

## {app} a-t-il besoin d'une permission Bluetooth ?
Oui. {app} utilise le Bluetooth Low Energy (BLE) pour se connecter au casque BCI. Sur macOS, une invite de permission unique apparaît lors de la première tentative de scan. Sous Linux et Windows, aucune permission Bluetooth explicite n'est requise.

## Comment accorder l'autorisation Accessibilité sur macOS ?
Ouvrez Réglages Système → Confidentialité et sécurité → Accessibilité. Trouvez {app} dans la liste et activez l'interrupteur. Vous pouvez aussi cliquer sur « Ouvrir les réglages Accessibilité » dans l'onglet Autorisations de l'application.

## Que se passe-t-il si je refuse l'autorisation Accessibilité ?
Les horodatages d'activité clavier et souris ne seront pas enregistrés et resteront à zéro. Toutes les autres fonctionnalités - streaming EEG, puissances de bande, calibration, TTS, recherche - continuent de fonctionner normalement. Vous pouvez désactiver entièrement la fonctionnalité dans Réglages → Suivi d'activité.

## Puis-je révoquer des autorisations après les avoir accordées ?
Oui. Ouvrez Réglages Système → Confidentialité et sécurité → Accessibilité (ou Notifications) et désactivez {app}. La fonctionnalité concernée cesse immédiatement de fonctionner, sans redémarrage.
