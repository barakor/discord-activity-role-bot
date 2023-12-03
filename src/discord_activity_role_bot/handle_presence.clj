(ns discord-activity-role-bot.handle-presence
  (:require 
            [clojure.set :as set :refer [intersection difference]]
            [clojure.string :as string]

            [discord-activity-role-bot.handle-db :refer [db]]

            [discljord.messaging :refer [get-guild-member! add-guild-member-role! remove-guild-member-role!]]
            [com.rpl.specter :as s :refer [ALL]]))
            

(defn contains-subset [values-set subs-set]
  (->> subs-set
    (map (fn [subs] (remove nil? (map #(re-find (re-pattern subs) %) values-set)))) 
    (remove empty?)
    (apply concat)))


(defn update-user-roles [rest-connection event-guild-id user-id roles-to-add roles-to-remove]
    (println "event-guild-id: " event-guild-id)
    (println "user-id: " user-id)
    (println "roles-to-add: " roles-to-add)
    (println "roles-to-remove: " roles-to-remove)
    (println "")
    (list (doall (map #(add-guild-member-role! rest-connection event-guild-id user-id %) roles-to-add))
          (doall (map #(remove-guild-member-role! rest-connection event-guild-id user-id %) roles-to-remove))))
           

(defn presence-update [event-data rest-connection]
 (let [user-id (get-in event-data [:user :id])
       event-guild-id (:guild-id event-data)
       guild-roles-rules (get-in @db [event-guild-id :roles-rules])
       user-current-roles (set (:roles @(get-guild-member! rest-connection event-guild-id user-id)))
       activities-names (->> event-data 
                          (s/select [:activities s/ALL :name #(not= % "Custom Status")])
                          (map string/lower-case)
                          (set))

       supervised-roles-ids (set (keys guild-roles-rules))
       user-curent-supervised-roles (intersection user-current-roles supervised-roles-ids)
       
       anything-roles-rules (->> guild-roles-rules
                              (s/select [s/ALL 
                                         #(= :else (:type (second %)))]) 
                              (map #(hash-map (first %) (second %)))
                              (apply merge))
       relavent-roles-rules (->> guild-roles-rules
                              (s/select [s/ALL 
                                         #(not-empty (contains-subset activities-names (:activity-names (second %))))])   
                              (map #(hash-map (first %) (second %)))
                              (apply merge))
       new-roles-ids (->> (if (empty? relavent-roles-rules)
                            anything-roles-rules
                            relavent-roles-rules)
                          (keys)
                          (map name))
                          
       roles-to-remove (difference user-curent-supervised-roles new-roles-ids)
       roles-to-add (difference new-roles-ids user-curent-supervised-roles)]
 
     (println event-data)
     (println (str "user-current-roles: " user-current-roles))
     (println (str "supervised-roles-ids: " supervised-roles-ids))
     (println (str "user-curent-supervised-roles: " user-curent-supervised-roles))
     (println (str "activities-names: " activities-names))
     (update-user-roles rest-connection event-guild-id user-id roles-to-add roles-to-remove)))

