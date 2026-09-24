# Brief graphique — yunopack

À remettre tel quel à un assistant de design. Il est autonome : tout ce qu'il
faut savoir du projet y figure.

---

## Ce qu'est yunopack

Un outil en ligne de commande, doublé d'une petite interface web, qui
transforme un dépôt Git en application installable sur **YunoHost** — une
distribution Linux d'auto-hébergement.

On lui donne l'adresse d'un dépôt. Il lit ce que le projet décrit déjà de
lui-même (son `Dockerfile`, son `docker-compose.yml`, ses fichiers de projet),
en déduit comment l'installer, et produit un paquet. Quand une information lui
manque, il le dit au lieu de la deviner.

Le public : des gens qui auto-hébergent leurs services, souvent sur une petite
machine à la maison. Plutôt techniques, attachés à la sobriété et à
l'indépendance vis-à-vis des grandes plateformes.

**Le ton juste** : sobre, précis, artisanal. Pas de dégradé tape-à-l'œil, pas de
vocabulaire d'agence. L'outil inspire confiance parce qu'il est carré, pas
parce qu'il brille. Pensez à la documentation d'un bon outil Unix plutôt qu'à
une page d'accueil de start-up.

**Ce qu'il ne faut pas faire** : reprendre les codes de l'IA (cerveaux, circuits,
particules), ni ceux du conteneur (baleines, boîtes empilées) — l'outil lit des
Dockerfiles, il n'en exécute aucun.

---

## 1. La marque

Un symbole, à décliner. Pistes possibles, non limitatives :

- l'idée de **transformation** — un dépôt entre, un paquet sort ;
- l'idée de **vérification** — le passage par une série de portes ;
- un clin d'œil discret à YunoHost, dont le logo est un octogone orange
  (`#ED7B21`). S'en rapprocher signale la parenté ; le copier serait déplacé,
  yunopack n'est pas un projet officiel.

Contraintes fortes :

- **lisible à 24 px** : c'est la taille réelle dans le catalogue et l'onglet du
  navigateur. Un symbole à trois traits vaut mieux qu'une illustration ;
- **monochrome viable** : il doit tenir en une seule couleur, sans dégradé ;
- **carré**, sans marge intégrée — les contextes d'affichage en ajoutent.

### Fichiers attendus

| Fichier | Format | Contrainte |
|---|---|---|
| `logo.svg` | SVG | la source, tracés vectorisés, sans texte en police système |
| `logo-140.png` | PNG 140×140 | **format exact du catalogue YunoHost**, viser moins de 10 Ko |
| `favicon.svg` | SVG | version simplifiée, lisible à 16 px |
| `favicon-32.png` | PNG 32×32 | repli pour les navigateurs anciens |
| `logo-512.png` | PNG 512×512 | README et présentations |

---

## 2. L'interface web

Une page unique, servie par le binaire. On y colle une URL, on suit le travail
en direct, on récupère un paquet.

### Ce qu'elle doit montrer

Le pipeline compte **cinq étapes**, franchies dans l'ordre, chacune pouvant
échouer et arrêter la suite :

1. **analyse** — lecture du dépôt (« 655 fichiers · Go · source 2.3.3 »)
2. **faisabilité** — verdict et score (« FAISABLE — score 100/100 »)
3. **spécification** — les arbitrages de packaging
4. **génération** — le paquet est écrit (« 13 fichiers »)
5. **vérification** — conformité (« paquet conforme »)

Le serveur envoie des événements au fil de l'eau, chacun portant : l'étape, un
message, si c'est terminé, et si ça s'est bien passé. Plusieurs messages par
étape. Un échec doit être **lisible et explicite** — c'est la moitié de la
valeur de l'outil : il explique pourquoi il refuse.

Exemple d'échec réel à savoir afficher sans que ce soit laid :

> **faisabilité** — non faisable : BUILD002 : recette de construction
> introuvable ; DB002 : service sans équivalent YunoHost (meilisearch)

Et d'un arrêt en cours de route :

> **spécification** — 2 champs à compléter : runtime.execstart,
> runtime.database_binding

### Ce qui doit rester

- **un seul fichier HTML**, style et script compris. Il est incorporé au
  binaire ;
- **aucune ressource externe** : pas de CDN, pas de police distante, pas
  d'appel réseau. L'outil tourne chez des gens qui s'auto-hébergent
  précisément pour éviter cela. Polices système uniquement ;
- **thème clair et sombre**, suivant le réglage du système ;
- **utilisable sur téléphone** (400 px de large) ;
- **échappement** de tout ce qui vient du serveur avant insertion dans le
  document — une URL de dépôt est une entrée utilisateur.

### Ce qui peut changer

Tout le reste. La page actuelle (122 lignes) fonctionne mais reste rudimentaire :
un champ, un bouton, des lignes empilées. Il manque une vraie lecture de la
progression, une hiérarchie entre les étapes, et une mise en valeur du
résultat.

Deux écrans secondaires seraient utiles, alimentés par des commandes qui
existent déjà :

- **la liste de souhaits** : 422 applications que la communauté YunoHost
  attend, avec nom, description et dépôt. On clique, ça lance le pipeline ;
- **les alternatives** : à partir d'un nom de logiciel, celles qui sont
  auto-hébergeables et pas encore au catalogue, avec leur nombre d'étoiles et
  leur licence.

Livrer : `page.html` complet, prêt à remplacer l'actuel.

---

## 3. Les captures d'écran

Le catalogue YunoHost et son administration affichent une galerie.

- au moins une capture, en `.png` ou `.jpg`, dans `doc/screenshots/` ;
- **512 Ko pour l'ensemble du dossier** — au-delà, le vérificateur officiel
  proteste ;
- montrer une exécution réussie, avec les cinq étapes visibles.

---

## Palette de départ

Rien d'imposé, mais l'interface actuelle emploie ceci, et cela fonctionne :

```
clair   fond #fbfaf8   texte #1c1b19   atténué #6b6862   trait #e0ddd6
        accent #2b6b4f (vert forêt)    alerte #a33
sombre  fond #16171a   texte #e8e6e1   atténué #9a968e   trait #2c2e33
        accent #6fbf95                 alerte #e4756b
```

Le vert distingue de l'orange YunoHost tout en restant sobre. Libre à vous de
proposer mieux, à condition de tenir le contraste AA dans les deux thèmes.

---

## Où déposer

```
assets/brand/          logo.svg, logo-140.png, favicon.svg, favicon-32.png, logo-512.png
crates/yunopack-server/src/page.html      l'interface
packaging/yunopack_ynh/doc/screenshots/   les captures
```

Le logo 140×140 part ensuite dans `logos/yunopack.png` du dépôt
[YunoHost/apps](https://github.com/YunoHost/apps) au moment de la demande
d'entrée au catalogue.
