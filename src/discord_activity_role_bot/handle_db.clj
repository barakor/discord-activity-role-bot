(ns discord-activity-role-bot.handle-db
  (:require [clojure.edn :as edn]
            [clojure.pprint :refer [pprint]]  
            [clojure.set :as set]
            [clojure.string :as string]
            [com.rpl.specter :refer :all]))


(def db (atom nil))

(def db-file-path "db.edn")

(def db-comment "; {\n;   server-id {\n;     :server-name \"server name\"\n;     :roles-rules {\n;       role-id {\n;         :type :named-activity/:else\n;         :activity-names [\"a\" \"b\" \"c\"] \n;         :role-name \"role name\"\n;         :comment \"comment\"\n;       }\n;     }\n;   }\n;  }\n; \n")


(defn save-db! [db db-file-path]
 (with-open [w (clojure.java.io/writer db-file-path)]
  (binding [*out* w
            *print-length* false]
    (println db-comment)
    (pprint db))))

(defn read-db! [db-file-path] (edn/read-string (slurp db-file-path)))


(defn load-db! []
 (reset! db (read-db! db-file-path)))

