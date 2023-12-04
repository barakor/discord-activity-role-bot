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
    ; (println "event-guild-id: " event-guild-id)
    ; (println "user-id: " user-id)
    ; (println "roles-to-add: " roles-to-add)
    ; (println "roles-to-remove: " roles-to-remove)
    ; (println "")
    (do (doall (map #(@(remove-guild-member-role! rest-connection event-guild-id user-id %)) roles-to-remove))
        (doall (map #(add-guild-member-role! rest-connection event-guild-id user-id %) roles-to-add))))
           

(defn presence-update [event-data rest-connection]
 (let [user-id (get-in event-data [:user :id])
       event-guild-id (:guild-id event-data)
       guild-roles-rules (get-in @db [event-guild-id :roles-rules])
       activities-names (->> event-data 
                          (s/select [:activities s/ALL :name #(not= % "Custom Status")])
                          (map string/lower-case)
                          (set))

       supervised-roles-ids (set (keys guild-roles-rules))
       
       anything-roles-rules (->> guild-roles-rules
                              (s/select [s/ALL #(= :else (:type (second %)))]) 
                              (map first))

       relavent-roles-rules (->> guild-roles-rules
                              (s/select [s/ALL #(not-empty (contains-subset activities-names (:activity-names (second %))))])   
                              (map first))

       new-roles-ids (cond 
                       (empty? activities-names) #{}
                       (not-empty relavent-roles-rules) (set relavent-roles-rules)
                       :else (set anything-roles-rules))]

     (update-user-roles rest-connection event-guild-id user-id new-roles-ids supervised-roles-ids)))

