(ns discord-activity-role-bot.core
  (:require [clojure.edn :as edn]  
            [clojure.set :as set]
            [clojure.string :as string]
            [com.rpl.specter :refer :all]))




(defn load-db [] (edn/read-string (slurp "db.edn")))

(defn lower_names [db] 
  (transform [MAP-VALS MAP-VALS :names ALL] string/lower-case db))

(defn get-db [] 
 (->> (load-db)
  (lower_names)))

(defn update-rule [db server-id rule-id new-rule]
 (->>db)
 ())





(get-db)
(def db (load-db))


(update-in db
 ["609603874474426372" "1087888852280164372"]
 #(identity
   {:names ["quake","diabotical"] :comments ["Test"]}))
 

 