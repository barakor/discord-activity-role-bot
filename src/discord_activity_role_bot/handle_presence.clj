(ns discord-activity-role-bot.handle-presence
  (:require 
            [clojure.set :as set :refer [intersection difference]]
            [clojure.string :as string]

            [discord-activity-role-bot.handle-db :refer [db]]

            [discljord.messaging :refer [get-guild-member! add-guild-member-role! remove-guild-member-role!]]
            [com.rpl.specter :as s]))


(defn get-roles-to-update [guild-roles-rules user-current-roles activities-names]
  (let [supervised-roles-ids (set (keys guild-roles-rules))
        user-curent-supervised-roles (intersection user-current-roles supervised-roles-ids)
        
        anything-roles-rules (s/select [s/ALL #(= :else (:type (second %))) s/ALL] guild-roles-rules)
        relavent-roles-rules (s/select [s/ALL #(not-empty (intersection activities-names (:activity-names (second %)))) s/ALL] guild-roles-rules)

        new-roles-ids (->> (if (empty? relavent-roles-rules)
                             anything-roles-rules
                             relavent-roles-rules)
                           (keys)
                           (map name)
                           (set))
        roles-to-remove (set/difference user-curent-supervised-roles new-roles-ids)
        roles-to-add (set/difference new-roles-ids user-curent-supervised-roles)]

    (list roles-to-add roles-to-remove)))


(defn update-user-roles [rest-connection event-guild-id user-id roles-to-add roles-to-remove]
    (println "event-guild-id: " event-guild-id)
    (println "user-id: " user-id)
    (println "roles-to-add: " roles-to-add)
    (println "roles-to-remove: " roles-to-remove)
    (list (doall (map #(add-guild-member-role! rest-connection event-guild-id user-id %) roles-to-add))
          (doall (map #(remove-guild-member-role! rest-connection event-guild-id user-id %) roles-to-remove))))
           

(defn presence-update [event-data rest-connection]
 (println event-data)
 (let [user-id (get-in event-data [:user :id])
       event-guild-id (:guild-id event-data)
       guild-roles-rules (get-in @db [event-guild-id :roles-rules])
       user-current-roles (set (:roles (get-guild-member! rest-connection event-guild-id user-id)))
       activities-names (->> event-data 
                          (s/select [:activities s/ALL :name #(not= % "Custom Status")])
                          (map string/lower-case)
                          (set))
       supervised-roles-ids (set (keys guild-roles-rules))
       user-curent-supervised-roles (intersection user-current-roles supervised-roles-ids)
       
       anything-roles-rules (s/select [s/ALL #(= :else (:type (second %))) s/ALL] guild-roles-rules)
       relavent-roles-rules (s/select [s/ALL #(not-empty (intersection activities-names (:activity-names (second %)))) s/ALL] guild-roles-rules)

       new-roles-ids (->> (if (empty? relavent-roles-rules)
                            anything-roles-rules
                            relavent-roles-rules)
                          (keys)
                          (map name)
                          (set))
       roles-to-remove (set/difference user-curent-supervised-roles new-roles-ids)
       roles-to-add (set/difference new-roles-ids user-curent-supervised-roles)]

     (update-user-roles rest-connection event-guild-id user-id roles-to-add roles-to-remove)))

