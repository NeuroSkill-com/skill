# Fenêtre principale
La fenêtre principale est le tableau de bord. Elle affiche les données EEG en temps réel, l'état de l'appareil et la qualité du signal. Elle est toujours visible dans la barre de menus.

## Statut
La carte supérieure affiche l'état de connexion en direct de votre appareil BCI. Un anneau coloré et un badge indiquent si l'appareil est déconnecté, en recherche, connecté ou si le Bluetooth est désactivé.

## Batterie
Une barre de progression montrant la charge actuelle de la batterie du casque BCI connecté. La couleur passe du vert (élevé) à l'orange puis au rouge (faible).

## Qualité du signal
Quatre points colorés - un par électrode EEG (TP9, AF7, AF8, TP10). Vert = bon contact. Jaune = moyen. Rouge = mauvais. Gris = pas de signal.

## Grille des canaux EEG
Quatre cartes montrant la dernière valeur (en μV) de chaque canal, avec un code couleur correspondant au graphique.

## Durée & Échantillons
La durée compte les secondes depuis le début de la session. Les échantillons sont le nombre total d'échantillons EEG reçus.

## Enregistrement CSV
Un indicateur ENR affiche le nom du fichier CSV en cours d'écriture dans {dataDir}/. Les échantillons bruts sont enregistrés en continu.

## Puissances de bande
Un graphique en barres montrant la puissance relative dans chaque bande de fréquence EEG standard : Delta, Theta, Alpha, Beta et Gamma.

## Asymétrie Alpha Frontale (FAA)
Une jauge ancrée au centre montrant l'index FAA en temps réel : ln(AF8 α) - ln(AF7 α). Les valeurs positives indiquent une puissance alpha fronto-droite plus élevée, associée à une motivation d'approche de l'hémisphère gauche. Les valeurs négatives indiquent une tendance au retrait. La valeur est lissée par une moyenne mobile exponentielle et varie typiquement de -1 à +1. La FAA est stockée avec chaque époque d'embedding de 5 secondes dans eeg.sqlite.

## Ondes EEG
Un graphique défilant du signal EEG filtré pour les quatre canaux. Sous chaque forme d'onde se trouve un spectrogramme.

## Utilisation GPU
Un petit graphique tout en haut de la fenêtre principale montrant l'utilisation de l'encodeur et du décodeur GPU. Visible uniquement lorsque l'encodeur d'embedding EEG est actif. Permet de vérifier que le pipeline wgpu fonctionne.

# États de l'icône de la barre

## Gris - Déconnecté
Le Bluetooth est activé ; aucun appareil BCI n'est connecté.

## Orange - Recherche
Recherche d'un appareil BCI ou tentative de connexion.

## Vert - Connecté
Transmission de données EEG en direct depuis votre appareil BCI.

## Rouge - Bluetooth désactivé
Le Bluetooth est désactivé. Aucune recherche ni connexion n'est possible.

# Communauté
Rejoignez la communauté Discord NeuroSkill pour poser des questions, partager vos retours et échanger avec d'autres utilisateurs et développeurs.
