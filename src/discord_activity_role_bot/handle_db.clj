(ns discord-activity-role-bot.handle-db
  (:require [clojure.edn :as edn]
            [clojure.java.io :refer [writer]]  
            [clojure.pprint :refer [pprint]]
            [com.rpl.specter :as s]
            [discljord.messaging :refer [get-guild!]]))


(def db (atom nil))

(def db-file-path "db.edn")

(def db-comment "; {\n;   server-id {\n;     :name \"server name\"\n;     :roles-rules {\n;       role-id {\n;         :type :named-activity/:else\n;         :activity-names [\"a\" \"b\" \"c\"] \n;         :role-name \"role name\"\n;         :comment \"comment\"\n;       }\n;     }\n;   }\n;  }\n; \n")



(defn- write-db! [db db-file-path]
  (with-open [w (writer db-file-path)]
   (binding [*out* w
             *print-length* false]
     (println db-comment)
     (pprint db))))

(defn save-db! []
  (write-db! @db db-file-path))


(defn- read-db! [db-file-path] (edn/read-string (slurp db-file-path)))


(defn load-db! []
  (reset! db (read-db! db-file-path)))


(defn- name-stuff [rest-connection [guild-id roles-map]]
  (let [guild-data @(get-guild! rest-connection guild-id)]
    (if-let [guild-name (:name guild-data)]
      (let [roles-names (apply merge (map #(hash-map  (:id %)  (:name %)) (:roles guild-data)))
            name-roles  (fn [roles-names [role-id role-map]] (list role-id (assoc role-map :name (get roles-names role-id))))
            named-roles-map  (->> roles-map
                               (#(assoc % :name guild-name))
                               (s/transform [:roles-rules s/ALL] (partial name-roles roles-names)))]
        (list guild-id named-roles-map))
      (list guild-id roles-map))))
        
(defn update-db-with-names [rest-connection]
  (reset! db (s/transform [s/ALL ] (partial name-stuff rest-connection) @db)))