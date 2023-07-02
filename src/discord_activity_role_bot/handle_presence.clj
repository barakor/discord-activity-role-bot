(ns discord-activity-role-bot.handle-presence
  (:require 
            [clojure.core.async :as async :refer [close!]]
            [discljord.messaging :as discord-rest]
            [clojure.set :as set]
            [clojure.string :as string]))
            


(defn get-anything-roles [guild-roles-rules]
  (filter (fn [[_ role-rules]]
            (empty? (get role-rules "names")))
          guild-roles-rules))

(defn get-relavent-roles [guild-roles-rules activities-names]
  (filter (fn [[role-id role-rules]]
            (->> role-rules
                 (#(get % "names"))
                 (map string/lower-case)
                 (set)
                 (set/intersection activities-names)
                 (seq)))
          guild-roles-rules))

(defn get-roles-to-update [db user-current-roles event-guild-id activities-names]
  (let [guild-roles-rules (get db event-guild-id)
        supervised-roles-ids (->> guild-roles-rules (keys) (map name) (set))
        user-curent-supervised-roles (set/intersection user-current-roles supervised-roles-ids)
        anything-roles-rules (if (seq activities-names)
                               (get-anything-roles guild-roles-rules)
                               #{})
        relavent-roles-rules (get-relavent-roles guild-roles-rules activities-names)
        new-roles-ids (->> (if (seq relavent-roles-rules)
                             relavent-roles-rules
                             anything-roles-rules)
                           (keys)
                           (map name)
                           (set))
        roles-to-remove (set/difference user-curent-supervised-roles new-roles-ids)
        roles-to-add (set/difference new-roles-ids user-curent-supervised-roles)]
        
    (println "roles-to-add: " roles-to-add)
    (println "roles-to-remove: " roles-to-remove)
    (list roles-to-add roles-to-remove)))

(defn update-user-roles [event-guild-id user-id roles-to-add roles-to-remove]
  (let [role-update (fn [f] (partial f (:rest @state) event-guild-id user-id))]
    (list (doall #((role-update discord-rest/add-guild-member-role!) %) roles-to-add)
          (doall #((role-update discord-rest/remove-guild-member-role!) %) roles-to-remove))))


(defn presence-update [event-data rest-connection db]
 (let [user-id (get-in event-data [:user :id])
         event-guild-id (:guild-id event-data)
         user-current-roles (->> event-data (:roles) (set))
         activities-names (->> event-data
                                (:activities)
                                (map :name)
                                (map string/lower-case)
                                (set)
                                (#(set/difference % #{"custom status"})))
         [roles-to-add roles-to-remove] (get-roles-to-update user-current-roles event-guild-id activities-names)]
     (update-user-roles event-guild-id user-id roles-to-add roles-to-remove)))
