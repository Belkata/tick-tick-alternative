# JNA looks up its own classes and the UniFFI interfaces by reflection.
-keep class com.sun.jna.** { *; }
-keep class * implements com.sun.jna.** { *; }
-keep class dev.tickalt.core.** { *; }
-dontwarn java.awt.**

# Glance runs on WorkManager, which creates several classes by reflection. R8 full mode
# (the AGP 9 default) no longer keeps default constructors implicitly, so the libraries'
# own rules aren't enough. Without these: a crash on start (Room database), then a
# widget stuck on "loading" (input merger / Glance's session worker).
-keep class * extends androidx.room.RoomDatabase { <init>(); }
-keep class androidx.work.impl.WorkDatabase_Impl { *; }
-keep class * extends androidx.work.InputMerger { <init>(); }
-keep class * extends androidx.work.ListenableWorker {
    <init>(android.content.Context, androidx.work.WorkerParameters);
}
# Glance creates widget action callbacks by class name.
-keep class * implements androidx.glance.appwidget.action.ActionCallback { <init>(); }
