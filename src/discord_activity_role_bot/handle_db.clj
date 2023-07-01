(ns discord-activity-role-bot.core
  (:require [clojure.edn :as edn]  
            [clojure.set :as set]
            [clojure.string :as string]
            [com.rpl.specter :refer :all]))


(def db-file-path "db.edn")

(def db-comment "; {server-id {\n; 	role-id {\n; 		:names [\"a\" \"b\" \"c\"] \n; 		:comments [\"comment\"]}}}\n")
(println db-comment)

(defn save-db [db]
 (with-open [w (clojure.java.io/writer db-file-path)]
  (binding [*out* w
            *print-length* false]
    (println db-comment)
    (pprint db))))

(defn load-db [] (edn/read-string (slurp db-file-path)))

(defn lower_names [db] 
  (transform [MAP-VALS MAP-VALS :names ALL] string/lower-case db))

(defn get-db [] 
 (->> (load-db)
  (lower_names)))

(defn update-rule [db server-id rule-id new-rule]
 (let [updated-db (update-in db [server-id rule-id] (fn [old-rule] new-rule))]
      updated-db)) 

(defn add-names-to-rule [db server-id rule-id names]
 (let [updated-db (transform [server-id rule-id :names] #(concat % (map string/lower-case names)) db)]
      updated-db)) 


(defn remove-names-from-rule [db server-id rule-id names]
 (let [has-names? (fn [rule-names] 
                   (filter 
                    (fn [rule-name] 
                     (some #(not= % rule-name) 
                      names)) 
                    rule-names))
       updated-db (transform [server-id rule-id :names] has-names?  db)]
      updated-db)) 

(defn update-rule-names [db server-id rule-id new-names]
 (let [updated-db (update-in db [server-id rule-id :names] (fn [old-names] (map string/lower-case new-names)))]
      updated-db)) 



(update-rule-names db
          "609603874474426372" "1087888852280164372" ["qUAke", "New", "bazooka"])



(remove-names-from-rule db
          "609603874474426372" "1087888852280164372" ["quake"])

(some #(= % "quake") ["quake" "diabotical"])



(get-db)
(def db (load-db))


(def ndb (update-rule db
          "609603874474426372" "1087888852280164372" {:names ["quake","diabotical"] :comments ["SUCK"]}))    

ndb
(save-db ndb)



(identity db)

(pr db)


