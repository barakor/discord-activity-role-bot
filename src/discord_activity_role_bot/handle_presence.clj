(ns discord-activity-role-bot.handle-presence
  (:require 
            [clojure.set :as set :refer [intersection difference]]
            [clojure.string :as string]

            [discljord.messaging :as discord-rest :refer [get-guild-member! add-guild-member-role! remove-guild-member-role!]]

            [discord-activity-role-bot.handle-db :refer [db]]

            [com.rpl.specter :as s :refer [ALL]]))
            

(defn contains-subset [values-set subs-set]
  (->> subs-set
    (map (fn [subs] (remove nil? (map #(re-find (re-pattern subs) %) values-set)))) 
    (remove empty?)
    (apply concat)))


(defn update-user-roles [rest-connection event-guild-id user-id roles-to-add roles-to-remove]
    (when (not-empty roles-to-remove)
      (doall (map #(deref (remove-guild-member-role! rest-connection event-guild-id user-id %)) roles-to-remove)))
    (when (not-empty roles-to-add)
      (doall (map #(add-guild-member-role! rest-connection event-guild-id user-id %) roles-to-add))))
           

(defn presence-update [event-data rest-connection]
 (let [user-id (get-in event-data [:user :id])
       event-guild-id (:guild-id event-data)
       guild-roles-rules (get-in @db [event-guild-id :roles-rules])
       supervised-roles-ids (set (keys guild-roles-rules))]

   (when (not-empty supervised-roles-ids)
     (let [user-current-roles (set (:roles @(discord-rest/get-guild-member! rest-connection event-guild-id user-id)))
           user-current-supervised-roles (set/intersection supervised-roles-ids user-current-roles)
           activities-names (->> event-data 
                              (s/select [:activities s/ALL :name #(not= % "Custom Status")])
                              (map string/lower-case)
                              (set))
           anything-roles-rules (->> guild-roles-rules
                                  (s/select [s/ALL #(= :else (:type (second %)))]) 
                                  (map first))
           relavent-roles-rules (->> guild-roles-rules
                                  (s/select [s/ALL #(not-empty (contains-subset activities-names (:activity-names (second %))))])   
                                  (map first))
           new-roles-ids   (cond 
                             (empty? activities-names) #{}
                             (not-empty relavent-roles-rules) (set relavent-roles-rules)
                             :else (set anything-roles-rules))
           roles-to-add    (set/difference new-roles-ids user-current-supervised-roles)
           roles-to-remove (set/difference user-current-supervised-roles new-roles-ids)]


       (when (or (not-empty roles-to-add) (not-empty roles-to-remove))
         (println "event-guild-id: " event-guild-id 
                "\n"
                "user-id: " user-id
                "\n"
                "activities-names: " activities-names
                "\n"
                "roles-to-remove: " roles-to-remove
                "\n"
                "roles-to-add: " roles-to-add 
                "\n")
         (update-user-roles rest-connection event-guild-id user-id roles-to-add roles-to-remove))))))


